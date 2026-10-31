//! Backend Vulkan de la traduction Direct3D.
//!
//! Plus proche sémantiquement de Direct3D 12 qu'OpenGL : files de commandes,
//! mémoire explicite, barrières. Utilisé par `d3d12.rs` quand
//! `gpu_caps::select_backend` le retient ; sinon OpenGL prend le relais.
//!
//! Choix de simplicité robustes :
//! - toutes les images vivent en layout `GENERAL` (valide pour copie,
//!   effacement, rendu) ; une barrière D3D12 devient une barrière mémoire
//!   globale, correcte quel que soit l'état demandé ;
//! - l'exécution est synchrone : `submit` attend la fin du GPU, ce qui rend
//!   les fences D3D12 exactes sans thread de surveillance ;
//! - rendu dynamique Vulkan 1.3 (pas d'objets render pass).

use crate::dlls::gpu_caps;
use crate::dlls::spirv;
use crate::dlls::vk_bindings::*;
use crate::{log_info, log_warn};
use std::ffi::{c_char, c_void};
use std::sync::{Arc, Mutex};

pub(crate) struct VkContext {
    pub instance: VkInstance,
    pub physical: VkPhysicalDevice,
    pub device: VkDevice,
    pub queue: VkQueue,
    pub family: u32,
    pub inst: VkInstanceFns,
    pub surf: Option<VkSurfaceFns>,
    pub dev: VkDeviceFns,
    pub dev13: VkDevice13Fns,
    pub swap: Option<VkSwapchainFns>,
    mem_props: VkPhysicalDeviceMemoryProperties,
    pool: VkCommandPool,
    cmd: VkCommandBuffer,
    fence: VkFence,
    /// Sérialise l'usage de l'unique tampon de commandes.
    lock: Mutex<()>,
    pub device_name: String,
    pipelines: Mutex<Vec<(PipelineKey, VkPipeline)>>,
    pipeline_layout: VkPipelineLayout,
    shaders: [VkShaderModule; 4],
}

unsafe impl Send for VkContext {}
unsafe impl Sync for VkContext {}

pub(crate) struct VkBufferAlloc {
    pub buffer: VkBuffer,
    pub memory: VkDeviceMemory,
    pub size: u64,
    pub mapped: *mut u8,
}

pub(crate) struct VkImageAlloc {
    pub image: VkImage,
    pub memory: VkDeviceMemory,
    pub view: VkImageView,
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub depth: bool,
}

unsafe impl Send for VkBufferAlloc {}
unsafe impl Send for VkImageAlloc {}

/// Clé de cache d'un pipeline passe-plat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PipelineKey {
    pub color_format: u32,
    /// (location, format Vulkan, offset) ; binding 0 seulement.
    pub attributes: Vec<(u32, u32, u32)>,
    pub with_color: bool,
}

/// Format DXGI → format Vulkan (sous-ensemble courant), `VK_FORMAT_UNDEFINED` sinon.
pub(crate) fn dxgi_to_vk(format: u32) -> u32 {
    match format {
        2 => VK_FORMAT_R32G32B32A32_SFLOAT,
        6 => VK_FORMAT_R32G32B32_SFLOAT,
        10 => VK_FORMAT_R16G16B16A16_SFLOAT,
        16 => VK_FORMAT_R32G32_SFLOAT,
        28 => VK_FORMAT_R8G8B8A8_UNORM,
        29 => VK_FORMAT_R8G8B8A8_SRGB,
        40 => VK_FORMAT_D32_SFLOAT,
        41 => VK_FORMAT_R32_SFLOAT,
        42 => VK_FORMAT_R32_UINT,
        45 => VK_FORMAT_D24_UNORM_S8_UINT,
        57 => VK_FORMAT_R16_UINT,
        87 => VK_FORMAT_B8G8R8A8_UNORM,
        91 => VK_FORMAT_B8G8R8A8_SRGB,
        _ => VK_FORMAT_UNDEFINED,
    }
}

fn full_range(aspect: u32) -> VkImageSubresourceRange {
    VkImageSubresourceRange {
        aspectMask: aspect,
        baseMipLevel: 0,
        levelCount: 1,
        baseArrayLayer: 0,
        layerCount: 1,
    }
}

impl VkContext {
    /// Crée instance, périphérique et file pour le périphérique retenu par
    /// les préférences. `None` à la moindre étape manquante.
    pub fn create() -> Option<Arc<VkContext>> {
        let caps = gpu_caps::caps();
        let prefs = gpu_caps::prefs();
        let chosen = gpu_caps::pick_vulkan_device(caps, &prefs)?;
        let vcaps = caps.vulkan.as_ref()?;
        let loader = loader()?;
        let global = global_fns()?;
        unsafe {
            let app = VkApplicationInfo {
                sType: VK_STRUCTURE_TYPE_APPLICATION_INFO,
                pNext: std::ptr::null(),
                pApplicationName: c"winlux".as_ptr(),
                applicationVersion: 1,
                pEngineName: c"winlux-d3d".as_ptr(),
                engineVersion: 1,
                apiVersion: VK_API_VERSION_1_3,
            };
            let mut exts: Vec<*const c_char> = Vec::new();
            if vcaps.has_xlib_surface {
                exts.push(c"VK_KHR_surface".as_ptr());
                exts.push(c"VK_KHR_xlib_surface".as_ptr());
            }
            let info = VkInstanceCreateInfo {
                sType: VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0,
                pApplicationInfo: &app,
                enabledLayerCount: 0,
                ppEnabledLayerNames: std::ptr::null(),
                enabledExtensionCount: exts.len() as u32,
                ppEnabledExtensionNames: exts.as_ptr(),
            };
            let mut instance = std::ptr::null_mut();
            if (global.create_instance)(&info, std::ptr::null(), &mut instance) != VK_SUCCESS {
                log_warn!("vulkan", "vkCreateInstance a échoué");
                return None;
            }
            let gipa = loader.get_instance_proc_addr;
            let inst = VkInstanceFns::load(instance, &|i, n| gipa(i, n))?;
            let surf = VkSurfaceFns::load(instance, &|i, n| gipa(i, n));
            let mut n = 0u32;
            (inst.enumerate_physical_devices)(instance, &mut n, std::ptr::null_mut());
            let mut pds = vec![std::ptr::null_mut(); n as usize];
            (inst.enumerate_physical_devices)(instance, &mut n, pds.as_mut_ptr());
            let Some(&physical) = pds.get(chosen.index) else {
                (inst.destroy_instance)(instance, std::ptr::null());
                return None;
            };
            let mut qn = 0u32;
            (inst.get_physical_device_queue_family_properties)(physical, &mut qn, std::ptr::null_mut());
            let mut qf = vec![VkQueueFamilyProperties::default(); qn as usize];
            (inst.get_physical_device_queue_family_properties)(physical, &mut qn, qf.as_mut_ptr());
            let Some(family) = qf
                .iter()
                .position(|q| q.queueFlags & VK_QUEUE_GRAPHICS_BIT != 0)
                .map(|f| f as u32)
            else {
                (inst.destroy_instance)(instance, std::ptr::null());
                return None;
            };
            let prio = 1.0f32;
            let qinfo = VkDeviceQueueCreateInfo {
                sType: VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0,
                queueFamilyIndex: family,
                queueCount: 1,
                pQueuePriorities: &prio,
            };
            let mut eds = VkPhysicalDeviceExtendedDynamicStateFeaturesEXT {
                sType: VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_EXTENDED_DYNAMIC_STATE_FEATURES_EXT,
                pNext: std::ptr::null_mut(),
                extendedDynamicState: 1,
            };
            let dynr = VkPhysicalDeviceDynamicRenderingFeatures {
                sType: VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_DYNAMIC_RENDERING_FEATURES,
                pNext: &mut eds as *mut _ as *mut c_void,
                dynamicRendering: 1,
            };
            let dev_exts = [c"VK_KHR_swapchain".as_ptr()];
            let dinfo = VkDeviceCreateInfo {
                sType: VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
                pNext: &dynr as *const _ as *const c_void,
                flags: 0,
                queueCreateInfoCount: 1,
                pQueueCreateInfos: &qinfo,
                enabledLayerCount: 0,
                ppEnabledLayerNames: std::ptr::null(),
                enabledExtensionCount: dev_exts.len() as u32,
                ppEnabledExtensionNames: dev_exts.as_ptr(),
                pEnabledFeatures: std::ptr::null(),
            };
            let mut device = std::ptr::null_mut();
            if (inst.create_device)(physical, &dinfo, std::ptr::null(), &mut device) != VK_SUCCESS {
                log_warn!("vulkan", "vkCreateDevice a échoué, repli OpenGL");
                (inst.destroy_instance)(instance, std::ptr::null());
                return None;
            }
            let gdpa = inst.get_device_proc_addr;
            let (Some(dev), Some(dev13)) = (
                VkDeviceFns::load(device, &|d, n| gdpa(d, n)),
                VkDevice13Fns::load(device, &|d, n| gdpa(d, n)),
            ) else {
                log_warn!("vulkan", "fonctions Vulkan 1.3 manquantes, repli OpenGL");
                (inst.destroy_instance)(instance, std::ptr::null());
                return None;
            };
            let swap = VkSwapchainFns::load(device, &|d, n| gdpa(d, n));
            let mut queue = std::ptr::null_mut();
            (dev.get_device_queue)(device, family, 0, &mut queue);
            let mut mem_props: VkPhysicalDeviceMemoryProperties = std::mem::zeroed();
            (inst.get_physical_device_memory_properties)(physical, &mut mem_props);

            let pinfo = VkCommandPoolCreateInfo {
                sType: VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT,
                queueFamilyIndex: family,
            };
            let mut pool = 0;
            (dev.create_command_pool)(device, &pinfo, std::ptr::null(), &mut pool);
            let ainfo = VkCommandBufferAllocateInfo {
                sType: VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
                pNext: std::ptr::null(),
                commandPool: pool,
                level: VK_COMMAND_BUFFER_LEVEL_PRIMARY,
                commandBufferCount: 1,
            };
            let mut cmd = std::ptr::null_mut();
            (dev.allocate_command_buffers)(device, &ainfo, &mut cmd);
            let finfo = VkFenceCreateInfo {
                sType: VK_STRUCTURE_TYPE_FENCE_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0,
            };
            let mut fence = 0;
            (dev.create_fence)(device, &finfo, std::ptr::null(), &mut fence);
            if pool == 0 || cmd.is_null() || fence == 0 {
                (dev.destroy_device)(device, std::ptr::null());
                (inst.destroy_instance)(instance, std::ptr::null());
                return None;
            }
            let layout_info = VkPipelineLayoutCreateInfo {
                sType: VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0,
                setLayoutCount: 0,
                pSetLayouts: std::ptr::null(),
                pushConstantRangeCount: 0,
                pPushConstantRanges: std::ptr::null(),
            };
            let mut pipeline_layout = 0;
            (dev.create_pipeline_layout)(device, &layout_info, std::ptr::null(), &mut pipeline_layout);
            let mut shaders = [0; 4];
            for (i, code) in [
                spirv::passthrough_vertex(false),
                spirv::passthrough_vertex(true),
                spirv::passthrough_fragment(false),
                spirv::passthrough_fragment(true),
            ]
            .iter()
            .enumerate()
            {
                let sinfo = VkShaderModuleCreateInfo {
                    sType: VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
                    pNext: std::ptr::null(),
                    flags: 0,
                    codeSize: code.len() * 4,
                    pCode: code.as_ptr(),
                };
                (dev.create_shader_module)(device, &sinfo, std::ptr::null(), &mut shaders[i]);
            }
            log_info!(
                "vulkan",
                "backend Vulkan prêt sur « {} » (API {})",
                chosen.name,
                gpu_caps::format_version(chosen.api_version)
            );
            Some(Arc::new(VkContext {
                instance,
                physical,
                device,
                queue,
                family,
                inst,
                surf,
                dev,
                dev13,
                swap,
                mem_props,
                pool,
                cmd,
                fence,
                lock: Mutex::new(()),
                device_name: chosen.name,
                pipelines: Mutex::new(Vec::new()),
                pipeline_layout,
                shaders,
            }))
        }
    }

    fn memory_type(&self, bits: u32, wanted: u32) -> Option<u32> {
        (0..self.mem_props.memoryTypeCount.min(32)).find(|&i| {
            bits & (1 << i) != 0
                && self.mem_props.memoryTypes[i as usize].propertyFlags & wanted == wanted
        })
    }

    unsafe fn allocate(&self, req: &VkMemoryRequirements, host: bool) -> Option<VkDeviceMemory> {
        let wanted = if host {
            VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT
        } else {
            VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT
        };
        let index = self
            .memory_type(req.memoryTypeBits, wanted)
            .or_else(|| self.memory_type(req.memoryTypeBits, 0))?;
        let info = VkMemoryAllocateInfo {
            sType: VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
            pNext: std::ptr::null(),
            allocationSize: req.size,
            memoryTypeIndex: index,
        };
        let mut mem = 0;
        ((self.dev.allocate_memory)(self.device, &info, std::ptr::null(), &mut mem) == VK_SUCCESS)
            .then_some(mem)
    }

    /// Tampon Vulkan. Les tampons « hôte » sont mappés en permanence, ce qui
    /// correspond aux tas UPLOAD/READBACK de D3D12.
    pub fn create_buffer(&self, size: u64, host: bool) -> Option<VkBufferAlloc> {
        let size = size.max(4);
        unsafe {
            let info = VkBufferCreateInfo {
                sType: VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0,
                size,
                usage: VK_BUFFER_USAGE_TRANSFER_SRC_BIT
                    | VK_BUFFER_USAGE_TRANSFER_DST_BIT
                    | VK_BUFFER_USAGE_VERTEX_BUFFER_BIT
                    | VK_BUFFER_USAGE_INDEX_BUFFER_BIT
                    | VK_BUFFER_USAGE_UNIFORM_BUFFER_BIT,
                sharingMode: VK_SHARING_MODE_EXCLUSIVE,
                queueFamilyIndexCount: 0,
                pQueueFamilyIndices: std::ptr::null(),
            };
            let mut buffer = 0;
            if (self.dev.create_buffer)(self.device, &info, std::ptr::null(), &mut buffer) != VK_SUCCESS {
                return None;
            }
            let mut req = VkMemoryRequirements::default();
            (self.dev.get_buffer_memory_requirements)(self.device, buffer, &mut req);
            let Some(memory) = self.allocate(&req, host) else {
                (self.dev.destroy_buffer)(self.device, buffer, std::ptr::null());
                return None;
            };
            (self.dev.bind_buffer_memory)(self.device, buffer, memory, 0);
            let mut mapped = std::ptr::null_mut();
            if host {
                (self.dev.map_memory)(self.device, memory, 0, u64::MAX, 0, &mut mapped);
            }
            Some(VkBufferAlloc {
                buffer,
                memory,
                size,
                mapped: mapped as *mut u8,
            })
        }
    }

    pub fn destroy_buffer(&self, b: &VkBufferAlloc) {
        unsafe {
            (self.dev.destroy_buffer)(self.device, b.buffer, std::ptr::null());
            (self.dev.free_memory)(self.device, b.memory, std::ptr::null());
        }
    }

    /// Image 2D en layout GENERAL, avec sa vue.
    pub fn create_image(&self, width: u32, height: u32, format: u32) -> Option<VkImageAlloc> {
        if format == VK_FORMAT_UNDEFINED || width == 0 || height == 0 {
            return None;
        }
        let depth = matches!(format, VK_FORMAT_D32_SFLOAT | VK_FORMAT_D24_UNORM_S8_UINT);
        let aspect = if depth {
            VK_IMAGE_ASPECT_DEPTH_BIT
                | if format == VK_FORMAT_D24_UNORM_S8_UINT {
                    VK_IMAGE_ASPECT_STENCIL_BIT
                } else {
                    0
                }
        } else {
            VK_IMAGE_ASPECT_COLOR_BIT
        };
        unsafe {
            let info = VkImageCreateInfo {
                sType: VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0,
                imageType: VK_IMAGE_TYPE_2D,
                format,
                extent: VkExtent3D {
                    width,
                    height,
                    depth: 1,
                },
                mipLevels: 1,
                arrayLayers: 1,
                samples: VK_SAMPLE_COUNT_1_BIT,
                tiling: VK_IMAGE_TILING_OPTIMAL,
                usage: VK_IMAGE_USAGE_TRANSFER_SRC_BIT
                    | VK_IMAGE_USAGE_TRANSFER_DST_BIT
                    | VK_IMAGE_USAGE_SAMPLED_BIT
                    | if depth {
                        VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT
                    } else {
                        VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT
                    },
                sharingMode: VK_SHARING_MODE_EXCLUSIVE,
                queueFamilyIndexCount: 0,
                pQueueFamilyIndices: std::ptr::null(),
                initialLayout: VK_IMAGE_LAYOUT_UNDEFINED,
            };
            let mut image = 0;
            if (self.dev.create_image)(self.device, &info, std::ptr::null(), &mut image) != VK_SUCCESS {
                return None;
            }
            let mut req = VkMemoryRequirements::default();
            (self.dev.get_image_memory_requirements)(self.device, image, &mut req);
            let Some(memory) = self.allocate(&req, false) else {
                (self.dev.destroy_image)(self.device, image, std::ptr::null());
                return None;
            };
            (self.dev.bind_image_memory)(self.device, image, memory, 0);
            let vinfo = VkImageViewCreateInfo {
                sType: VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0,
                image,
                viewType: VK_IMAGE_VIEW_TYPE_2D,
                format,
                components: VkComponentMapping::default(),
                subresourceRange: full_range(aspect),
            };
            let mut view = 0;
            (self.dev.create_image_view)(self.device, &vinfo, std::ptr::null(), &mut view);
            let img = VkImageAlloc {
                image,
                memory,
                view,
                width,
                height,
                format,
                depth,
            };
            // Passage unique UNDEFINED → GENERAL.
            self.submit(|cmd| self.layout_barrier(cmd, image, aspect, VK_IMAGE_LAYOUT_UNDEFINED, VK_IMAGE_LAYOUT_GENERAL));
            Some(img)
        }
    }

    pub fn destroy_image(&self, i: &VkImageAlloc) {
        unsafe {
            (self.dev.destroy_image_view)(self.device, i.view, std::ptr::null());
            (self.dev.destroy_image)(self.device, i.image, std::ptr::null());
            (self.dev.free_memory)(self.device, i.memory, std::ptr::null());
        }
    }

    pub fn layout_barrier(&self, cmd: VkCommandBuffer, image: VkImage, aspect: u32, old: u32, new: u32) {
        let b = VkImageMemoryBarrier {
            sType: VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
            pNext: std::ptr::null(),
            srcAccessMask: VK_ACCESS_MEMORY_WRITE_BIT,
            dstAccessMask: VK_ACCESS_MEMORY_READ_BIT | VK_ACCESS_MEMORY_WRITE_BIT,
            oldLayout: old,
            newLayout: new,
            srcQueueFamilyIndex: u32::MAX,
            dstQueueFamilyIndex: u32::MAX,
            image,
            subresourceRange: full_range(aspect),
        };
        unsafe {
            (self.dev.cmd_pipeline_barrier)(
                cmd,
                VK_PIPELINE_STAGE_ALL_COMMANDS_BIT,
                VK_PIPELINE_STAGE_ALL_COMMANDS_BIT,
                0,
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
                1,
                &b,
            );
        }
    }

    /// Barrière mémoire globale (traduction de `ResourceBarrier` D3D12).
    pub fn memory_barrier(&self, cmd: VkCommandBuffer) {
        let b = VkMemoryBarrier {
            sType: VK_STRUCTURE_TYPE_MEMORY_BARRIER,
            pNext: std::ptr::null(),
            srcAccessMask: VK_ACCESS_MEMORY_WRITE_BIT,
            dstAccessMask: VK_ACCESS_MEMORY_READ_BIT | VK_ACCESS_MEMORY_WRITE_BIT,
        };
        unsafe {
            (self.dev.cmd_pipeline_barrier)(
                cmd,
                VK_PIPELINE_STAGE_ALL_COMMANDS_BIT,
                VK_PIPELINE_STAGE_ALL_COMMANDS_BIT,
                0,
                1,
                &b,
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
            );
        }
    }

    /// Enregistre `record`, soumet et attend la fin. `false` si le GPU a
    /// refusé (périphérique perdu…) : l'appelant le signale sans planter.
    pub fn submit<F: FnOnce(VkCommandBuffer)>(&self, record: F) -> bool {
        self.submit_with(record, &[], &[])
    }

    pub fn submit_with<F: FnOnce(VkCommandBuffer)>(
        &self,
        record: F,
        wait: &[VkSemaphore],
        signal: &[VkSemaphore],
    ) -> bool {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            (self.dev.reset_command_buffer)(self.cmd, 0);
            let begin = VkCommandBufferBeginInfo {
                sType: VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
                pNext: std::ptr::null(),
                flags: VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT,
                pInheritanceInfo: std::ptr::null(),
            };
            if (self.dev.begin_command_buffer)(self.cmd, &begin) != VK_SUCCESS {
                return false;
            }
            record(self.cmd);
            if (self.dev.end_command_buffer)(self.cmd) != VK_SUCCESS {
                return false;
            }
            let stages = vec![VK_PIPELINE_STAGE_ALL_COMMANDS_BIT; wait.len()];
            let info = VkSubmitInfo {
                sType: VK_STRUCTURE_TYPE_SUBMIT_INFO,
                pNext: std::ptr::null(),
                waitSemaphoreCount: wait.len() as u32,
                pWaitSemaphores: wait.as_ptr(),
                pWaitDstStageMask: stages.as_ptr(),
                commandBufferCount: 1,
                pCommandBuffers: &self.cmd,
                signalSemaphoreCount: signal.len() as u32,
                pSignalSemaphores: signal.as_ptr(),
            };
            if (self.dev.queue_submit)(self.queue, 1, &info, self.fence) != VK_SUCCESS {
                log_warn!("vulkan", "vkQueueSubmit a échoué");
                return false;
            }
            let ok = (self.dev.wait_for_fences)(self.device, 1, &self.fence, 1, 5_000_000_000)
                == VK_SUCCESS;
            (self.dev.reset_fences)(self.device, 1, &self.fence);
            ok
        }
    }

    pub fn clear_color(&self, cmd: VkCommandBuffer, img: &VkImageAlloc, rgba: [f32; 4]) {
        let c = VkClearColorValue { float32: rgba };
        let r = full_range(VK_IMAGE_ASPECT_COLOR_BIT);
        unsafe {
            (self.dev.cmd_clear_color_image)(cmd, img.image, VK_IMAGE_LAYOUT_GENERAL, &c, 1, &r);
        }
    }

    pub fn clear_depth(&self, cmd: VkCommandBuffer, img: &VkImageAlloc, depth: f32, stencil: u32, flags: u32) {
        let mut aspect = 0;
        if flags & 1 != 0 {
            aspect |= VK_IMAGE_ASPECT_DEPTH_BIT;
        }
        if flags & 2 != 0 && img.format == VK_FORMAT_D24_UNORM_S8_UINT {
            aspect |= VK_IMAGE_ASPECT_STENCIL_BIT;
        }
        if aspect == 0 {
            return;
        }
        let v = VkClearDepthStencilValue { depth, stencil };
        let r = full_range(aspect);
        unsafe {
            (self.dev.cmd_clear_depth_stencil_image)(cmd, img.image, VK_IMAGE_LAYOUT_GENERAL, &v, 1, &r);
        }
    }

    pub fn copy_buffer(&self, cmd: VkCommandBuffer, src: VkBuffer, so: u64, dst: VkBuffer, d_o: u64, size: u64) {
        let region = VkBufferCopy {
            srcOffset: so,
            dstOffset: d_o,
            size,
        };
        unsafe { (self.dev.cmd_copy_buffer)(cmd, src, dst, 1, &region) }
    }

    pub fn copy_buffer_to_image(&self, cmd: VkCommandBuffer, src: VkBuffer, offset: u64, row_pixels: u32, img: &VkImageAlloc) {
        let region = VkBufferImageCopy {
            bufferOffset: offset,
            bufferRowLength: row_pixels,
            bufferImageHeight: 0,
            imageSubresource: VkImageSubresourceLayers {
                aspectMask: VK_IMAGE_ASPECT_COLOR_BIT,
                mipLevel: 0,
                baseArrayLayer: 0,
                layerCount: 1,
            },
            imageOffset: VkOffset3D::default(),
            imageExtent: VkExtent3D {
                width: img.width,
                height: img.height,
                depth: 1,
            },
        };
        unsafe { (self.dev.cmd_copy_buffer_to_image)(cmd, src, img.image, VK_IMAGE_LAYOUT_GENERAL, 1, &region) }
    }

    pub fn copy_image_to_buffer(&self, cmd: VkCommandBuffer, img: &VkImageAlloc, dst: VkBuffer, offset: u64, row_pixels: u32) {
        let region = VkBufferImageCopy {
            bufferOffset: offset,
            bufferRowLength: row_pixels,
            bufferImageHeight: 0,
            imageSubresource: VkImageSubresourceLayers {
                aspectMask: VK_IMAGE_ASPECT_COLOR_BIT,
                mipLevel: 0,
                baseArrayLayer: 0,
                layerCount: 1,
            },
            imageOffset: VkOffset3D::default(),
            imageExtent: VkExtent3D {
                width: img.width,
                height: img.height,
                depth: 1,
            },
        };
        unsafe { (self.dev.cmd_copy_image_to_buffer)(cmd, img.image, VK_IMAGE_LAYOUT_GENERAL, dst, 1, &region) }
    }

    pub fn blit(&self, cmd: VkCommandBuffer, src: VkImage, sw: u32, sh: u32, src_layout: u32, dst: VkImage, dw: u32, dh: u32, dst_layout: u32) {
        let layers = VkImageSubresourceLayers {
            aspectMask: VK_IMAGE_ASPECT_COLOR_BIT,
            mipLevel: 0,
            baseArrayLayer: 0,
            layerCount: 1,
        };
        let region = VkImageBlit {
            srcSubresource: layers,
            srcOffsets: [
                VkOffset3D::default(),
                VkOffset3D {
                    x: sw as i32,
                    y: sh as i32,
                    z: 1,
                },
            ],
            dstSubresource: layers,
            dstOffsets: [
                VkOffset3D::default(),
                VkOffset3D {
                    x: dw as i32,
                    y: dh as i32,
                    z: 1,
                },
            ],
        };
        unsafe { (self.dev.cmd_blit_image)(cmd, src, src_layout, dst, dst_layout, 1, &region, VK_FILTER_LINEAR) }
    }

    /// Pipeline passe-plat (mis en cache) pour une cible et une disposition
    /// de sommets données.
    pub fn pipeline(&self, key: &PipelineKey) -> Option<VkPipeline> {
        let mut cache = self.pipelines.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, p)) = cache.iter().find(|(k, _)| k == key) {
            return Some(*p);
        }
        let with_color = key.with_color && key.attributes.iter().any(|a| a.0 == 1);
        let vs = self.shaders[with_color as usize];
        let fs = self.shaders[2 + with_color as usize];
        if vs == 0 || fs == 0 {
            return None;
        }
        unsafe {
            let stages = [
                VkPipelineShaderStageCreateInfo {
                    sType: VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
                    pNext: std::ptr::null(),
                    flags: 0,
                    stage: VK_SHADER_STAGE_VERTEX_BIT,
                    module: vs,
                    pName: c"main".as_ptr(),
                    pSpecializationInfo: std::ptr::null(),
                },
                VkPipelineShaderStageCreateInfo {
                    sType: VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
                    pNext: std::ptr::null(),
                    flags: 0,
                    stage: VK_SHADER_STAGE_FRAGMENT_BIT,
                    module: fs,
                    pName: c"main".as_ptr(),
                    pSpecializationInfo: std::ptr::null(),
                },
            ];
            let binding = VkVertexInputBindingDescription {
                binding: 0,
                stride: 0, // dynamique
                inputRate: VK_VERTEX_INPUT_RATE_VERTEX,
            };
            let attrs: Vec<VkVertexInputAttributeDescription> = key
                .attributes
                .iter()
                .filter(|a| a.0 == 0 || (with_color && a.0 == 1))
                .map(|&(location, format, offset)| VkVertexInputAttributeDescription {
                    location,
                    binding: 0,
                    format,
                    offset,
                })
                .collect();
            let vi = VkPipelineVertexInputStateCreateInfo {
                sType: VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0,
                vertexBindingDescriptionCount: 1,
                pVertexBindingDescriptions: &binding,
                vertexAttributeDescriptionCount: attrs.len() as u32,
                pVertexAttributeDescriptions: attrs.as_ptr(),
            };
            let ia = VkPipelineInputAssemblyStateCreateInfo {
                sType: VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0,
                topology: VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
                primitiveRestartEnable: 0,
            };
            let vp = VkPipelineViewportStateCreateInfo {
                sType: VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0,
                viewportCount: 1,
                pViewports: std::ptr::null(),
                scissorCount: 1,
                pScissors: std::ptr::null(),
            };
            let rs = VkPipelineRasterizationStateCreateInfo {
                sType: VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0,
                depthClampEnable: 0,
                rasterizerDiscardEnable: 0,
                polygonMode: VK_POLYGON_MODE_FILL,
                cullMode: VK_CULL_MODE_NONE,
                frontFace: VK_FRONT_FACE_CLOCKWISE,
                depthBiasEnable: 0,
                depthBiasConstantFactor: 0.0,
                depthBiasClamp: 0.0,
                depthBiasSlopeFactor: 0.0,
                lineWidth: 1.0,
            };
            let ms = VkPipelineMultisampleStateCreateInfo {
                sType: VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0,
                rasterizationSamples: VK_SAMPLE_COUNT_1_BIT,
                sampleShadingEnable: 0,
                minSampleShading: 0.0,
                pSampleMask: std::ptr::null(),
                alphaToCoverageEnable: 0,
                alphaToOneEnable: 0,
            };
            let att = VkPipelineColorBlendAttachmentState {
                colorWriteMask: VK_COLOR_COMPONENT_RGBA,
                ..Default::default()
            };
            let cb = VkPipelineColorBlendStateCreateInfo {
                sType: VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0,
                logicOpEnable: 0,
                logicOp: 0,
                attachmentCount: 1,
                pAttachments: &att,
                blendConstants: [0.0; 4],
            };
            let dyn_states = [
                VK_DYNAMIC_STATE_VIEWPORT,
                VK_DYNAMIC_STATE_SCISSOR,
                VK_DYNAMIC_STATE_PRIMITIVE_TOPOLOGY,
                VK_DYNAMIC_STATE_VERTEX_INPUT_BINDING_STRIDE,
            ];
            let ds = VkPipelineDynamicStateCreateInfo {
                sType: VK_STRUCTURE_TYPE_PIPELINE_DYNAMIC_STATE_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0,
                dynamicStateCount: dyn_states.len() as u32,
                pDynamicStates: dyn_states.as_ptr(),
            };
            let rendering = VkPipelineRenderingCreateInfo {
                sType: VK_STRUCTURE_TYPE_PIPELINE_RENDERING_CREATE_INFO,
                pNext: std::ptr::null(),
                viewMask: 0,
                colorAttachmentCount: 1,
                pColorAttachmentFormats: &key.color_format,
                depthAttachmentFormat: VK_FORMAT_UNDEFINED,
                stencilAttachmentFormat: VK_FORMAT_UNDEFINED,
            };
            let info = VkGraphicsPipelineCreateInfo {
                sType: VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO,
                pNext: &rendering as *const _ as *const c_void,
                flags: 0,
                stageCount: 2,
                pStages: stages.as_ptr(),
                pVertexInputState: &vi,
                pInputAssemblyState: &ia,
                pTessellationState: std::ptr::null(),
                pViewportState: &vp,
                pRasterizationState: &rs,
                pMultisampleState: &ms,
                pDepthStencilState: std::ptr::null(),
                pColorBlendState: &cb,
                pDynamicState: &ds,
                layout: self.pipeline_layout,
                renderPass: 0,
                subpass: 0,
                basePipelineHandle: 0,
                basePipelineIndex: -1,
            };
            let mut pipeline = 0;
            if (self.dev.create_graphics_pipelines)(self.device, 0, 1, &info, std::ptr::null(), &mut pipeline)
                != VK_SUCCESS
            {
                log_warn!("vulkan", "création du pipeline passe-plat refusée");
                return None;
            }
            cache.push((key.clone(), pipeline));
            Some(pipeline)
        }
    }

    /// Dessin dans `target` avec le pipeline passe-plat.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        cmd: VkCommandBuffer,
        target: &VkImageAlloc,
        pipeline: VkPipeline,
        topology: u32,
        viewport: VkViewport,
        vertex: (VkBuffer, u64, u64),
        index: Option<(VkBuffer, u64, u32)>,
        counts: (u32, u32, u32, i32, u32),
    ) {
        unsafe {
            let attachment = VkRenderingAttachmentInfo {
                sType: VK_STRUCTURE_TYPE_RENDERING_ATTACHMENT_INFO,
                pNext: std::ptr::null(),
                imageView: target.view,
                imageLayout: VK_IMAGE_LAYOUT_GENERAL,
                resolveMode: 0,
                resolveImageView: 0,
                resolveImageLayout: 0,
                loadOp: VK_ATTACHMENT_LOAD_OP_LOAD,
                storeOp: VK_ATTACHMENT_STORE_OP_STORE,
                clearValue: VkClearValue {
                    color: VkClearColorValue { float32: [0.0; 4] },
                },
            };
            let area = VkRect2D {
                offset: VkOffset2D::default(),
                extent: VkExtent2D {
                    width: target.width,
                    height: target.height,
                },
            };
            let info = VkRenderingInfo {
                sType: VK_STRUCTURE_TYPE_RENDERING_INFO,
                pNext: std::ptr::null(),
                flags: 0,
                renderArea: area,
                layerCount: 1,
                viewMask: 0,
                colorAttachmentCount: 1,
                pColorAttachments: &attachment,
                pDepthAttachment: std::ptr::null(),
                pStencilAttachment: std::ptr::null(),
            };
            (self.dev13.cmd_begin_rendering)(cmd, &info);
            (self.dev.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, pipeline);
            (self.dev.cmd_set_viewport)(cmd, 0, 1, &viewport);
            (self.dev.cmd_set_scissor)(cmd, 0, 1, &area);
            (self.dev13.cmd_set_primitive_topology)(cmd, topology);
            let (vb, vo, stride) = vertex;
            (self.dev13.cmd_bind_vertex_buffers2)(cmd, 0, 1, &vb, &vo, std::ptr::null(), &stride);
            let (count, instances, first, base, first_instance) = counts;
            match index {
                Some((ib, io, ty)) => {
                    (self.dev.cmd_bind_index_buffer)(cmd, ib, io, ty);
                    (self.dev.cmd_draw_indexed)(cmd, count, instances, first, base, first_instance);
                }
                None => (self.dev.cmd_draw)(cmd, count, instances, first, first_instance),
            }
            (self.dev13.cmd_end_rendering)(cmd);
        }
    }
}

impl Drop for VkContext {
    fn drop(&mut self) {
        unsafe {
            (self.dev.device_wait_idle)(self.device);
            for (_, p) in self.pipelines.lock().unwrap_or_else(|e| e.into_inner()).drain(..) {
                (self.dev.destroy_pipeline)(self.device, p, std::ptr::null());
            }
            for s in self.shaders {
                if s != 0 {
                    (self.dev.destroy_shader_module)(self.device, s, std::ptr::null());
                }
            }
            (self.dev.destroy_pipeline_layout)(self.device, self.pipeline_layout, std::ptr::null());
            (self.dev.destroy_fence)(self.device, self.fence, std::ptr::null());
            (self.dev.free_command_buffers)(self.device, self.pool, 1, &self.cmd);
            (self.dev.destroy_command_pool)(self.device, self.pool, std::ptr::null());
            (self.dev.destroy_device)(self.device, std::ptr::null());
            (self.inst.destroy_instance)(self.instance, std::ptr::null());
        }
    }
}

// ── Présentation dans une fenêtre X11 ──────────────────────────────────────

pub(crate) struct VkPresenter {
    ctx: Arc<VkContext>,
    display: *mut c_void,
    surface: VkSurfaceKHR,
    swapchain: VkSwapchainKHR,
    images: Vec<VkImage>,
    extent: VkExtent2D,
    format: u32,
    acquire: VkSemaphore,
    rendered: VkSemaphore,
    window: u64,
    vsync: bool,
}

unsafe impl Send for VkPresenter {}

impl VkPresenter {
    /// Surface Xlib + swapchain sur la fenêtre X `window`. `None` si la
    /// surface n'est pas présentable (l'appelant garde alors OpenGL).
    pub fn create(ctx: Arc<VkContext>, window: u64, width: u32, height: u32, vsync: bool) -> Option<Self> {
        let surf = ctx.surf.as_ref()?;
        ctx.swap.as_ref()?;
        let glx = crate::dlls::gl_bindings::glx()?;
        if !glx.available || window == 0 {
            return None;
        }
        unsafe {
            let display = (glx.XOpenDisplay)(std::ptr::null());
            if display.is_null() {
                return None;
            }
            let info = VkXlibSurfaceCreateInfoKHR {
                sType: VK_STRUCTURE_TYPE_XLIB_SURFACE_CREATE_INFO_KHR,
                pNext: std::ptr::null(),
                flags: 0,
                dpy: display,
                window,
            };
            let mut surface = 0;
            if (surf.create_xlib_surface)(ctx.instance, &info, std::ptr::null(), &mut surface) != VK_SUCCESS {
                (glx.XCloseDisplay)(display);
                return None;
            }
            let mut supported = 0;
            (surf.get_surface_support)(ctx.physical, ctx.family, surface, &mut supported);
            if supported == 0 {
                (surf.destroy_surface)(ctx.instance, surface, std::ptr::null());
                (glx.XCloseDisplay)(display);
                return None;
            }
            let sinfo = VkSemaphoreCreateInfo {
                sType: VK_STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO,
                pNext: std::ptr::null(),
                flags: 0,
            };
            let (mut acquire, mut rendered) = (0, 0);
            (ctx.dev.create_semaphore)(ctx.device, &sinfo, std::ptr::null(), &mut acquire);
            (ctx.dev.create_semaphore)(ctx.device, &sinfo, std::ptr::null(), &mut rendered);
            let mut p = VkPresenter {
                ctx,
                display,
                surface,
                swapchain: 0,
                images: Vec::new(),
                extent: VkExtent2D { width, height },
                format: VK_FORMAT_B8G8R8A8_UNORM,
                acquire,
                rendered,
                window,
                vsync,
            };
            if !p.rebuild(width, height) {
                return None;
            }
            Some(p)
        }
    }

    pub fn rebuild(&mut self, width: u32, height: u32) -> bool {
        let ctx = self.ctx.clone();
        let (Some(surf), Some(swap)) = (ctx.surf.as_ref(), ctx.swap.as_ref()) else {
            return false;
        };
        unsafe {
            (ctx.dev.device_wait_idle)(ctx.device);
            let mut caps = VkSurfaceCapabilitiesKHR::default();
            if (surf.get_surface_capabilities)(ctx.physical, self.surface, &mut caps) != VK_SUCCESS {
                return false;
            }
            let mut n = 0u32;
            (surf.get_surface_formats)(ctx.physical, self.surface, &mut n, std::ptr::null_mut());
            let mut formats = vec![VkSurfaceFormatKHR::default(); n as usize];
            (surf.get_surface_formats)(ctx.physical, self.surface, &mut n, formats.as_mut_ptr());
            formats.truncate(n as usize);
            let chosen = formats
                .iter()
                .find(|f| f.format == VK_FORMAT_B8G8R8A8_UNORM)
                .or_else(|| formats.iter().find(|f| f.format == VK_FORMAT_R8G8B8A8_UNORM))
                .or(formats.first())
                .copied();
            let Some(fmt) = chosen else { return false };
            let mut pn = 0u32;
            (surf.get_surface_present_modes)(ctx.physical, self.surface, &mut pn, std::ptr::null_mut());
            let mut modes = vec![0u32; pn as usize];
            (surf.get_surface_present_modes)(ctx.physical, self.surface, &mut pn, modes.as_mut_ptr());
            // FIFO est toujours disponible ; sans synchro verticale on préfère MAILBOX puis IMMEDIATE.
            let mode = if self.vsync {
                VK_PRESENT_MODE_FIFO_KHR
            } else if modes.contains(&VK_PRESENT_MODE_MAILBOX_KHR) {
                VK_PRESENT_MODE_MAILBOX_KHR
            } else if modes.contains(&VK_PRESENT_MODE_IMMEDIATE_KHR) {
                VK_PRESENT_MODE_IMMEDIATE_KHR
            } else {
                VK_PRESENT_MODE_FIFO_KHR
            };
            let extent = if caps.currentExtent.width != u32::MAX {
                caps.currentExtent
            } else {
                VkExtent2D {
                    width: width.clamp(caps.minImageExtent.width.max(1), caps.maxImageExtent.width.max(1)),
                    height: height.clamp(caps.minImageExtent.height.max(1), caps.maxImageExtent.height.max(1)),
                }
            };
            if extent.width == 0 || extent.height == 0 {
                return false; // fenêtre réduite : on réessaiera au prochain Present
            }
            let mut count = caps.minImageCount.max(2);
            if caps.maxImageCount != 0 {
                count = count.min(caps.maxImageCount);
            }
            let alpha = if caps.supportedCompositeAlpha & VK_COMPOSITE_ALPHA_OPAQUE_BIT_KHR != 0 {
                VK_COMPOSITE_ALPHA_OPAQUE_BIT_KHR
            } else {
                VK_COMPOSITE_ALPHA_INHERIT_BIT_KHR
            };
            let info = VkSwapchainCreateInfoKHR {
                sType: VK_STRUCTURE_TYPE_SWAPCHAIN_CREATE_INFO_KHR,
                pNext: std::ptr::null(),
                flags: 0,
                surface: self.surface,
                minImageCount: count,
                imageFormat: fmt.format,
                imageColorSpace: fmt.colorSpace,
                imageExtent: extent,
                imageArrayLayers: 1,
                imageUsage: VK_IMAGE_USAGE_TRANSFER_DST_BIT | VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT,
                imageSharingMode: VK_SHARING_MODE_EXCLUSIVE,
                queueFamilyIndexCount: 0,
                pQueueFamilyIndices: std::ptr::null(),
                preTransform: caps.currentTransform,
                compositeAlpha: alpha,
                presentMode: mode,
                clipped: 1,
                oldSwapchain: self.swapchain,
            };
            let mut sc = 0;
            if (swap.create_swapchain)(ctx.device, &info, std::ptr::null(), &mut sc) != VK_SUCCESS {
                log_warn!("vulkan", "vkCreateSwapchainKHR a échoué");
                return false;
            }
            if self.swapchain != 0 {
                (swap.destroy_swapchain)(ctx.device, self.swapchain, std::ptr::null());
            }
            self.swapchain = sc;
            let mut ni = 0u32;
            (swap.get_swapchain_images)(ctx.device, sc, &mut ni, std::ptr::null_mut());
            self.images = vec![0; ni as usize];
            (swap.get_swapchain_images)(ctx.device, sc, &mut ni, self.images.as_mut_ptr());
            self.extent = extent;
            self.format = fmt.format;
            true
        }
    }

    /// Copie `src` (layout GENERAL) dans l'image suivante et la présente.
    pub fn present(&mut self, src: &VkImageAlloc) -> bool {
        let ctx = self.ctx.clone();
        let Some(swap) = ctx.swap.as_ref() else {
            return false;
        };
        if self.swapchain == 0 && !self.rebuild(self.extent.width, self.extent.height) {
            return false;
        }
        unsafe {
            let mut index = 0u32;
            let r = (swap.acquire_next_image)(ctx.device, self.swapchain, 1_000_000_000, self.acquire, 0, &mut index);
            if r == VK_ERROR_OUT_OF_DATE_KHR {
                self.rebuild(src.width, src.height);
                return false;
            }
            if r != VK_SUCCESS && r != VK_SUBOPTIMAL_KHR {
                return false;
            }
            let Some(&dst) = self.images.get(index as usize) else {
                return false;
            };
            let (dw, dh) = (self.extent.width, self.extent.height);
            let ok = ctx.submit_with(
                |cmd| {
                    ctx.layout_barrier(cmd, dst, VK_IMAGE_ASPECT_COLOR_BIT, VK_IMAGE_LAYOUT_UNDEFINED, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL);
                    ctx.blit(cmd, src.image, src.width, src.height, VK_IMAGE_LAYOUT_GENERAL, dst, dw, dh, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL);
                    ctx.layout_barrier(cmd, dst, VK_IMAGE_ASPECT_COLOR_BIT, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, VK_IMAGE_LAYOUT_PRESENT_SRC_KHR);
                },
                &[self.acquire],
                &[self.rendered],
            );
            if !ok {
                return false;
            }
            let info = VkPresentInfoKHR {
                sType: VK_STRUCTURE_TYPE_PRESENT_INFO_KHR,
                pNext: std::ptr::null(),
                waitSemaphoreCount: 1,
                pWaitSemaphores: &self.rendered,
                swapchainCount: 1,
                pSwapchains: &self.swapchain,
                pImageIndices: &index,
                pResults: std::ptr::null_mut(),
            };
            let r = (swap.queue_present)(ctx.queue, &info);
            if r == VK_ERROR_OUT_OF_DATE_KHR || r == VK_SUBOPTIMAL_KHR {
                self.rebuild(src.width, src.height);
            }
            (ctx.dev.queue_wait_idle)(ctx.queue);
            r == VK_SUCCESS || r == VK_SUBOPTIMAL_KHR
        }
    }

    pub fn window(&self) -> u64 {
        self.window
    }
}

impl Drop for VkPresenter {
    fn drop(&mut self) {
        let ctx = &self.ctx;
        unsafe {
            (ctx.dev.device_wait_idle)(ctx.device);
            if let Some(swap) = ctx.swap.as_ref() {
                if self.swapchain != 0 {
                    (swap.destroy_swapchain)(ctx.device, self.swapchain, std::ptr::null());
                }
            }
            (ctx.dev.destroy_semaphore)(ctx.device, self.acquire, std::ptr::null());
            (ctx.dev.destroy_semaphore)(ctx.device, self.rendered, std::ptr::null());
            if let Some(surf) = ctx.surf.as_ref() {
                (surf.destroy_surface)(ctx.instance, self.surface, std::ptr::null());
            }
            if let Some(glx) = crate::dlls::gl_bindings::glx() {
                (glx.XCloseDisplay)(self.display);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_dxgi_vers_vulkan() {
        assert_eq!(dxgi_to_vk(28), VK_FORMAT_R8G8B8A8_UNORM);
        assert_eq!(dxgi_to_vk(87), VK_FORMAT_B8G8R8A8_UNORM);
        assert_eq!(dxgi_to_vk(6), VK_FORMAT_R32G32B32_SFLOAT);
        assert_eq!(dxgi_to_vk(9999), VK_FORMAT_UNDEFINED);
    }

    /// Exécute un vrai rendu hors écran si un pilote Vulkan est présent
    /// (llvmpipe suffit) : effacement, triangle passe-plat, relecture.
    /// Sans Vulkan, le test vérifie seulement que la création échoue proprement.
    #[test]
    fn rendu_hors_ecran_ou_repli() {
        std::env::set_var("WINLUX_GFX_BACKEND", "vulkan");
        let Some(ctx) = VkContext::create() else {
            return;
        };
        let img = ctx.create_image(16, 16, VK_FORMAT_R8G8B8A8_UNORM).expect("image");
        // Triangle couvrant tout l'écran.
        let verts: [f32; 9] = [-1.0, -1.0, 0.0, 3.0, -1.0, 0.0, -1.0, 3.0, 0.0];
        let vb = ctx.create_buffer(36, true).expect("vb");
        unsafe { std::ptr::copy_nonoverlapping(verts.as_ptr() as *const u8, vb.mapped, 36) };
        let rb = ctx.create_buffer(16 * 16 * 4, true).expect("rb");
        let key = PipelineKey {
            color_format: VK_FORMAT_R8G8B8A8_UNORM,
            attributes: vec![(0, VK_FORMAT_R32G32B32_SFLOAT, 0)],
            with_color: false,
        };
        let pipe = ctx.pipeline(&key).expect("pipeline");
        assert!(ctx.submit(|cmd| {
            ctx.clear_color(cmd, &img, [1.0, 0.0, 0.0, 1.0]);
            ctx.memory_barrier(cmd);
        }));
        // Avant le dessin : rouge.
        assert!(ctx.submit(|cmd| ctx.copy_image_to_buffer(cmd, &img, rb.buffer, 0, 16)));
        let px = unsafe { std::slice::from_raw_parts(rb.mapped, 4) };
        assert_eq!(px, &[255, 0, 0, 255]);
        let vp = VkViewport {
            x: 0.0,
            y: 0.0,
            width: 16.0,
            height: 16.0,
            minDepth: 0.0,
            maxDepth: 1.0,
        };
        assert!(ctx.submit(|cmd| {
            ctx.draw(cmd, &img, pipe, VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST, vp, (vb.buffer, 0, 12), None, (3, 1, 0, 0, 0));
            ctx.memory_barrier(cmd);
            ctx.copy_image_to_buffer(cmd, &img, rb.buffer, 0, 16);
        }));
        let px = unsafe { std::slice::from_raw_parts(rb.mapped, 16 * 16 * 4) };
        // Le shader passe-plat sans couleur écrit du blanc partout.
        assert!(px.chunks(4).all(|p| p == [255, 255, 255, 255]));
        ctx.destroy_buffer(&vb);
        ctx.destroy_buffer(&rb);
        ctx.destroy_image(&img);
    }
}
