//! Chargement dynamique de Vulkan via `dlopen(libvulkan.so.1)`.
//!
//! Aucune dépendance de compilation : si le chargeur Vulkan n'est pas
//! installé, ou si un symbole manque, les fonctions de chargement renvoient
//! `None` et l'appelant se replie sur OpenGL. Les structures sont déclarées à
//! la main d'après la spécification Vulkan (tailles vérifiées par les tests).
#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::{c_char, c_void, CStr};
use std::sync::OnceLock;

pub(crate) type VkResult = i32;
pub(crate) type VkBool32 = u32;
pub(crate) type VkFlags = u32;
pub(crate) type VkDeviceSize = u64;

pub(crate) type VkInstance = *mut c_void;
pub(crate) type VkPhysicalDevice = *mut c_void;
pub(crate) type VkDevice = *mut c_void;
pub(crate) type VkQueue = *mut c_void;
pub(crate) type VkCommandBuffer = *mut c_void;
// Objets « non dispatchables » : des entiers 64 bits.
pub(crate) type VkSurfaceKHR = u64;
pub(crate) type VkSwapchainKHR = u64;
pub(crate) type VkImage = u64;
pub(crate) type VkImageView = u64;
pub(crate) type VkBuffer = u64;
pub(crate) type VkDeviceMemory = u64;
pub(crate) type VkCommandPool = u64;
pub(crate) type VkFence = u64;
pub(crate) type VkSemaphore = u64;
pub(crate) type VkShaderModule = u64;
pub(crate) type VkPipelineLayout = u64;
pub(crate) type VkPipeline = u64;

pub(crate) const VK_SUCCESS: VkResult = 0;
pub(crate) const VK_NOT_READY: VkResult = 1;
pub(crate) const VK_TIMEOUT: VkResult = 2;
pub(crate) const VK_INCOMPLETE: VkResult = 5;
pub(crate) const VK_SUBOPTIMAL_KHR: VkResult = 1000001003;
pub(crate) const VK_ERROR_OUT_OF_DATE_KHR: VkResult = -1000001004;

pub(crate) const fn vk_make_api_version(variant: u32, major: u32, minor: u32, patch: u32) -> u32 {
    (variant << 29) | (major << 22) | (minor << 12) | patch
}
pub(crate) const VK_API_VERSION_1_0: u32 = vk_make_api_version(0, 1, 0, 0);
pub(crate) const VK_API_VERSION_1_1: u32 = vk_make_api_version(0, 1, 1, 0);
pub(crate) const VK_API_VERSION_1_2: u32 = vk_make_api_version(0, 1, 2, 0);
pub(crate) const VK_API_VERSION_1_3: u32 = vk_make_api_version(0, 1, 3, 0);
pub(crate) const fn version_major(v: u32) -> u32 {
    (v >> 22) & 0x7f
}
pub(crate) const fn version_minor(v: u32) -> u32 {
    (v >> 12) & 0x3ff
}
pub(crate) const fn version_patch(v: u32) -> u32 {
    v & 0xfff
}

// ── sType ───────────────────────────────────────────────────────────────────
pub(crate) const VK_STRUCTURE_TYPE_APPLICATION_INFO: u32 = 0;
pub(crate) const VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO: u32 = 1;
pub(crate) const VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO: u32 = 2;
pub(crate) const VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO: u32 = 3;
pub(crate) const VK_STRUCTURE_TYPE_SUBMIT_INFO: u32 = 4;
pub(crate) const VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO: u32 = 5;
pub(crate) const VK_STRUCTURE_TYPE_FENCE_CREATE_INFO: u32 = 8;
pub(crate) const VK_STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO: u32 = 9;
pub(crate) const VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO: u32 = 12;
pub(crate) const VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO: u32 = 14;
pub(crate) const VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO: u32 = 15;
pub(crate) const VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO: u32 = 16;
pub(crate) const VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO: u32 = 18;
pub(crate) const VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO: u32 = 19;
pub(crate) const VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO: u32 = 20;
pub(crate) const VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO: u32 = 22;
pub(crate) const VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO: u32 = 23;
pub(crate) const VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO: u32 = 24;
pub(crate) const VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO: u32 = 26;
pub(crate) const VK_STRUCTURE_TYPE_PIPELINE_DYNAMIC_STATE_CREATE_INFO: u32 = 27;
pub(crate) const VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO: u32 = 28;
pub(crate) const VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO: u32 = 30;
pub(crate) const VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO: u32 = 39;
pub(crate) const VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO: u32 = 40;
pub(crate) const VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO: u32 = 42;
pub(crate) const VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER: u32 = 45;
pub(crate) const VK_STRUCTURE_TYPE_MEMORY_BARRIER: u32 = 46;
pub(crate) const VK_STRUCTURE_TYPE_SWAPCHAIN_CREATE_INFO_KHR: u32 = 1000001000;
pub(crate) const VK_STRUCTURE_TYPE_PRESENT_INFO_KHR: u32 = 1000001001;
pub(crate) const VK_STRUCTURE_TYPE_XLIB_SURFACE_CREATE_INFO_KHR: u32 = 1000004000;
pub(crate) const VK_STRUCTURE_TYPE_RENDERING_INFO: u32 = 1000044000;
pub(crate) const VK_STRUCTURE_TYPE_RENDERING_ATTACHMENT_INFO: u32 = 1000044001;
pub(crate) const VK_STRUCTURE_TYPE_PIPELINE_RENDERING_CREATE_INFO: u32 = 1000044002;
pub(crate) const VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_DYNAMIC_RENDERING_FEATURES: u32 = 1000044003;
pub(crate) const VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_EXTENDED_DYNAMIC_STATE_FEATURES_EXT: u32 =
    1000267000;

// ── Énumérations et masques utiles ─────────────────────────────────────────
pub(crate) const VK_PHYSICAL_DEVICE_TYPE_OTHER: u32 = 0;
pub(crate) const VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU: u32 = 1;
pub(crate) const VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU: u32 = 2;
pub(crate) const VK_PHYSICAL_DEVICE_TYPE_VIRTUAL_GPU: u32 = 3;
pub(crate) const VK_PHYSICAL_DEVICE_TYPE_CPU: u32 = 4;

pub(crate) const VK_QUEUE_GRAPHICS_BIT: u32 = 0x1;
pub(crate) const VK_QUEUE_COMPUTE_BIT: u32 = 0x2;
pub(crate) const VK_QUEUE_TRANSFER_BIT: u32 = 0x4;

pub(crate) const VK_FORMAT_UNDEFINED: u32 = 0;
pub(crate) const VK_FORMAT_R8G8B8A8_UNORM: u32 = 37;
pub(crate) const VK_FORMAT_R8G8B8A8_SRGB: u32 = 43;
pub(crate) const VK_FORMAT_B8G8R8A8_UNORM: u32 = 44;
pub(crate) const VK_FORMAT_B8G8R8A8_SRGB: u32 = 50;
pub(crate) const VK_FORMAT_R16_UINT: u32 = 74;
pub(crate) const VK_FORMAT_R16G16B16A16_SFLOAT: u32 = 97;
pub(crate) const VK_FORMAT_R32_UINT: u32 = 98;
pub(crate) const VK_FORMAT_R32_SFLOAT: u32 = 100;
pub(crate) const VK_FORMAT_R32G32_SFLOAT: u32 = 103;
pub(crate) const VK_FORMAT_R32G32B32_SFLOAT: u32 = 106;
pub(crate) const VK_FORMAT_R32G32B32A32_SFLOAT: u32 = 109;
pub(crate) const VK_FORMAT_D32_SFLOAT: u32 = 126;
pub(crate) const VK_FORMAT_D24_UNORM_S8_UINT: u32 = 129;
pub(crate) const VK_COLOR_SPACE_SRGB_NONLINEAR_KHR: u32 = 0;

pub(crate) const VK_IMAGE_USAGE_TRANSFER_SRC_BIT: u32 = 0x1;
pub(crate) const VK_IMAGE_USAGE_TRANSFER_DST_BIT: u32 = 0x2;
pub(crate) const VK_IMAGE_USAGE_SAMPLED_BIT: u32 = 0x4;
pub(crate) const VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT: u32 = 0x10;
pub(crate) const VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT: u32 = 0x20;

pub(crate) const VK_BUFFER_USAGE_TRANSFER_SRC_BIT: u32 = 0x1;
pub(crate) const VK_BUFFER_USAGE_TRANSFER_DST_BIT: u32 = 0x2;
pub(crate) const VK_BUFFER_USAGE_UNIFORM_BUFFER_BIT: u32 = 0x10;
pub(crate) const VK_BUFFER_USAGE_INDEX_BUFFER_BIT: u32 = 0x40;
pub(crate) const VK_BUFFER_USAGE_VERTEX_BUFFER_BIT: u32 = 0x80;

pub(crate) const VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT: u32 = 0x1;
pub(crate) const VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT: u32 = 0x2;
pub(crate) const VK_MEMORY_PROPERTY_HOST_COHERENT_BIT: u32 = 0x4;

pub(crate) const VK_IMAGE_LAYOUT_UNDEFINED: u32 = 0;
pub(crate) const VK_IMAGE_LAYOUT_GENERAL: u32 = 1;
pub(crate) const VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL: u32 = 2;
pub(crate) const VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL: u32 = 5;
pub(crate) const VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL: u32 = 6;
pub(crate) const VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL: u32 = 7;
pub(crate) const VK_IMAGE_LAYOUT_PRESENT_SRC_KHR: u32 = 1000001002;

pub(crate) const VK_IMAGE_ASPECT_COLOR_BIT: u32 = 0x1;
pub(crate) const VK_IMAGE_ASPECT_DEPTH_BIT: u32 = 0x2;
pub(crate) const VK_IMAGE_ASPECT_STENCIL_BIT: u32 = 0x4;

pub(crate) const VK_ACCESS_TRANSFER_READ_BIT: u32 = 0x800;
pub(crate) const VK_ACCESS_TRANSFER_WRITE_BIT: u32 = 0x1000;
pub(crate) const VK_ACCESS_MEMORY_READ_BIT: u32 = 0x8000;
pub(crate) const VK_ACCESS_MEMORY_WRITE_BIT: u32 = 0x10000;

pub(crate) const VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT: u32 = 0x1;
pub(crate) const VK_PIPELINE_STAGE_TRANSFER_BIT: u32 = 0x1000;
pub(crate) const VK_PIPELINE_STAGE_BOTTOM_OF_PIPE_BIT: u32 = 0x2000;
pub(crate) const VK_PIPELINE_STAGE_ALL_COMMANDS_BIT: u32 = 0x10000;
pub(crate) const VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT: u32 = 0x400;

pub(crate) const VK_SHARING_MODE_EXCLUSIVE: u32 = 0;
pub(crate) const VK_IMAGE_TYPE_2D: u32 = 1;
pub(crate) const VK_IMAGE_VIEW_TYPE_2D: u32 = 1;
pub(crate) const VK_IMAGE_TILING_OPTIMAL: u32 = 0;
pub(crate) const VK_SAMPLE_COUNT_1_BIT: u32 = 1;
pub(crate) const VK_COMMAND_BUFFER_LEVEL_PRIMARY: u32 = 0;
pub(crate) const VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT: u32 = 0x2;
pub(crate) const VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT: u32 = 0x1;
pub(crate) const VK_FENCE_CREATE_SIGNALED_BIT: u32 = 0x1;
pub(crate) const VK_PRESENT_MODE_IMMEDIATE_KHR: u32 = 0;
pub(crate) const VK_PRESENT_MODE_MAILBOX_KHR: u32 = 1;
pub(crate) const VK_PRESENT_MODE_FIFO_KHR: u32 = 2;
pub(crate) const VK_COMPOSITE_ALPHA_OPAQUE_BIT_KHR: u32 = 0x1;
pub(crate) const VK_COMPOSITE_ALPHA_INHERIT_BIT_KHR: u32 = 0x8;
pub(crate) const VK_SURFACE_TRANSFORM_IDENTITY_BIT_KHR: u32 = 0x1;
pub(crate) const VK_FILTER_LINEAR: u32 = 1;
pub(crate) const VK_INDEX_TYPE_UINT16: u32 = 0;
pub(crate) const VK_INDEX_TYPE_UINT32: u32 = 1;
pub(crate) const VK_PIPELINE_BIND_POINT_GRAPHICS: u32 = 0;
pub(crate) const VK_SHADER_STAGE_VERTEX_BIT: u32 = 0x1;
pub(crate) const VK_SHADER_STAGE_FRAGMENT_BIT: u32 = 0x10;
pub(crate) const VK_ATTACHMENT_LOAD_OP_LOAD: u32 = 0;
pub(crate) const VK_ATTACHMENT_LOAD_OP_CLEAR: u32 = 1;
pub(crate) const VK_ATTACHMENT_STORE_OP_STORE: u32 = 0;
pub(crate) const VK_VERTEX_INPUT_RATE_VERTEX: u32 = 0;
pub(crate) const VK_PRIMITIVE_TOPOLOGY_POINT_LIST: u32 = 0;
pub(crate) const VK_PRIMITIVE_TOPOLOGY_LINE_LIST: u32 = 1;
pub(crate) const VK_PRIMITIVE_TOPOLOGY_LINE_STRIP: u32 = 2;
pub(crate) const VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST: u32 = 3;
pub(crate) const VK_PRIMITIVE_TOPOLOGY_TRIANGLE_STRIP: u32 = 4;
pub(crate) const VK_POLYGON_MODE_FILL: u32 = 0;
pub(crate) const VK_CULL_MODE_NONE: u32 = 0;
pub(crate) const VK_FRONT_FACE_CLOCKWISE: u32 = 1;
pub(crate) const VK_COLOR_COMPONENT_RGBA: u32 = 0xf;
pub(crate) const VK_DYNAMIC_STATE_VIEWPORT: u32 = 0;
pub(crate) const VK_DYNAMIC_STATE_SCISSOR: u32 = 1;
pub(crate) const VK_DYNAMIC_STATE_PRIMITIVE_TOPOLOGY: u32 = 1000267002;
pub(crate) const VK_DYNAMIC_STATE_VERTEX_INPUT_BINDING_STRIDE: u32 = 1000267005;

// ── Structures ─────────────────────────────────────────────────────────────

#[repr(C)]
pub(crate) struct VkApplicationInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub pApplicationName: *const c_char,
    pub applicationVersion: u32,
    pub pEngineName: *const c_char,
    pub engineVersion: u32,
    pub apiVersion: u32,
}

#[repr(C)]
pub(crate) struct VkInstanceCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub pApplicationInfo: *const VkApplicationInfo,
    pub enabledLayerCount: u32,
    pub ppEnabledLayerNames: *const *const c_char,
    pub enabledExtensionCount: u32,
    pub ppEnabledExtensionNames: *const *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct VkExtensionProperties {
    pub extensionName: [c_char; 256],
    pub specVersion: u32,
}

impl VkExtensionProperties {
    pub fn name(&self) -> String {
        // SAFETY: le pilote garantit une chaîne terminée par NUL dans le tableau.
        unsafe { CStr::from_ptr(self.extensionName.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }
}

pub(crate) const ZERO_EXTENSION: VkExtensionProperties = VkExtensionProperties {
    extensionName: [0; 256],
    specVersion: 0,
};

/// `VkPhysicalDeviceProperties` (824 octets). Seuls les champs de tête sont
/// lus ; le reste (limites, propriétés sparse) est conservé brut.
#[repr(C, align(8))]
pub(crate) struct VkPhysicalDeviceProperties {
    pub apiVersion: u32,
    pub driverVersion: u32,
    pub vendorID: u32,
    pub deviceID: u32,
    pub deviceType: u32,
    pub deviceName: [c_char; 256],
    pub pipelineCacheUUID: [u8; 16],
    pub rest: [u8; 528],
}

impl VkPhysicalDeviceProperties {
    pub fn zeroed() -> Self {
        // SAFETY: structure de données brute, zéro est un état valide.
        unsafe { std::mem::zeroed() }
    }
    pub fn name(&self) -> String {
        let bytes: Vec<u8> = self
            .deviceName
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkExtent3D {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkExtent2D {
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkOffset2D {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkOffset3D {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkRect2D {
    pub offset: VkOffset2D,
    pub extent: VkExtent2D,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkQueueFamilyProperties {
    pub queueFlags: VkFlags,
    pub queueCount: u32,
    pub timestampValidBits: u32,
    pub minImageTransferGranularity: VkExtent3D,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkMemoryType {
    pub propertyFlags: VkFlags,
    pub heapIndex: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkMemoryHeap {
    pub size: VkDeviceSize,
    pub flags: VkFlags,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct VkPhysicalDeviceMemoryProperties {
    pub memoryTypeCount: u32,
    pub memoryTypes: [VkMemoryType; 32],
    pub memoryHeapCount: u32,
    pub memoryHeaps: [VkMemoryHeap; 16],
}

#[repr(C)]
pub(crate) struct VkDeviceQueueCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub queueFamilyIndex: u32,
    pub queueCount: u32,
    pub pQueuePriorities: *const f32,
}

#[repr(C)]
pub(crate) struct VkDeviceCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub queueCreateInfoCount: u32,
    pub pQueueCreateInfos: *const VkDeviceQueueCreateInfo,
    pub enabledLayerCount: u32,
    pub ppEnabledLayerNames: *const *const c_char,
    pub enabledExtensionCount: u32,
    pub ppEnabledExtensionNames: *const *const c_char,
    pub pEnabledFeatures: *const c_void,
}

#[repr(C)]
pub(crate) struct VkPhysicalDeviceDynamicRenderingFeatures {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub dynamicRendering: VkBool32,
}

#[repr(C)]
pub(crate) struct VkPhysicalDeviceExtendedDynamicStateFeaturesEXT {
    pub sType: u32,
    pub pNext: *mut c_void,
    pub extendedDynamicState: VkBool32,
}

#[repr(C)]
pub(crate) struct VkXlibSurfaceCreateInfoKHR {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub dpy: *mut c_void,
    pub window: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkSurfaceCapabilitiesKHR {
    pub minImageCount: u32,
    pub maxImageCount: u32,
    pub currentExtent: VkExtent2D,
    pub minImageExtent: VkExtent2D,
    pub maxImageExtent: VkExtent2D,
    pub maxImageArrayLayers: u32,
    pub supportedTransforms: VkFlags,
    pub currentTransform: u32,
    pub supportedCompositeAlpha: VkFlags,
    pub supportedUsageFlags: VkFlags,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkSurfaceFormatKHR {
    pub format: u32,
    pub colorSpace: u32,
}

#[repr(C)]
pub(crate) struct VkSwapchainCreateInfoKHR {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub surface: VkSurfaceKHR,
    pub minImageCount: u32,
    pub imageFormat: u32,
    pub imageColorSpace: u32,
    pub imageExtent: VkExtent2D,
    pub imageArrayLayers: u32,
    pub imageUsage: VkFlags,
    pub imageSharingMode: u32,
    pub queueFamilyIndexCount: u32,
    pub pQueueFamilyIndices: *const u32,
    pub preTransform: u32,
    pub compositeAlpha: u32,
    pub presentMode: u32,
    pub clipped: VkBool32,
    pub oldSwapchain: VkSwapchainKHR,
}

#[repr(C)]
pub(crate) struct VkPresentInfoKHR {
    pub sType: u32,
    pub pNext: *const c_void,
    pub waitSemaphoreCount: u32,
    pub pWaitSemaphores: *const VkSemaphore,
    pub swapchainCount: u32,
    pub pSwapchains: *const VkSwapchainKHR,
    pub pImageIndices: *const u32,
    pub pResults: *mut VkResult,
}

#[repr(C)]
pub(crate) struct VkCommandPoolCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub queueFamilyIndex: u32,
}

#[repr(C)]
pub(crate) struct VkCommandBufferAllocateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub commandPool: VkCommandPool,
    pub level: u32,
    pub commandBufferCount: u32,
}

#[repr(C)]
pub(crate) struct VkCommandBufferBeginInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub pInheritanceInfo: *const c_void,
}

#[repr(C)]
pub(crate) struct VkSubmitInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub waitSemaphoreCount: u32,
    pub pWaitSemaphores: *const VkSemaphore,
    pub pWaitDstStageMask: *const VkFlags,
    pub commandBufferCount: u32,
    pub pCommandBuffers: *const VkCommandBuffer,
    pub signalSemaphoreCount: u32,
    pub pSignalSemaphores: *const VkSemaphore,
}

#[repr(C)]
pub(crate) struct VkFenceCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
}

#[repr(C)]
pub(crate) struct VkSemaphoreCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkImageSubresourceRange {
    pub aspectMask: VkFlags,
    pub baseMipLevel: u32,
    pub levelCount: u32,
    pub baseArrayLayer: u32,
    pub layerCount: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkImageSubresourceLayers {
    pub aspectMask: VkFlags,
    pub mipLevel: u32,
    pub baseArrayLayer: u32,
    pub layerCount: u32,
}

#[repr(C)]
pub(crate) struct VkImageMemoryBarrier {
    pub sType: u32,
    pub pNext: *const c_void,
    pub srcAccessMask: VkFlags,
    pub dstAccessMask: VkFlags,
    pub oldLayout: u32,
    pub newLayout: u32,
    pub srcQueueFamilyIndex: u32,
    pub dstQueueFamilyIndex: u32,
    pub image: VkImage,
    pub subresourceRange: VkImageSubresourceRange,
}

#[repr(C)]
pub(crate) struct VkMemoryBarrier {
    pub sType: u32,
    pub pNext: *const c_void,
    pub srcAccessMask: VkFlags,
    pub dstAccessMask: VkFlags,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) union VkClearColorValue {
    pub float32: [f32; 4],
    pub uint32: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkClearDepthStencilValue {
    pub depth: f32,
    pub stencil: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) union VkClearValue {
    pub color: VkClearColorValue,
    pub depthStencil: VkClearDepthStencilValue,
}

#[repr(C)]
pub(crate) struct VkBufferCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub size: VkDeviceSize,
    pub usage: VkFlags,
    pub sharingMode: u32,
    pub queueFamilyIndexCount: u32,
    pub pQueueFamilyIndices: *const u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkMemoryRequirements {
    pub size: VkDeviceSize,
    pub alignment: VkDeviceSize,
    pub memoryTypeBits: u32,
}

#[repr(C)]
pub(crate) struct VkMemoryAllocateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub allocationSize: VkDeviceSize,
    pub memoryTypeIndex: u32,
}

#[repr(C)]
pub(crate) struct VkImageCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub imageType: u32,
    pub format: u32,
    pub extent: VkExtent3D,
    pub mipLevels: u32,
    pub arrayLayers: u32,
    pub samples: u32,
    pub tiling: u32,
    pub usage: VkFlags,
    pub sharingMode: u32,
    pub queueFamilyIndexCount: u32,
    pub pQueueFamilyIndices: *const u32,
    pub initialLayout: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkComponentMapping {
    pub r: u32,
    pub g: u32,
    pub b: u32,
    pub a: u32,
}

#[repr(C)]
pub(crate) struct VkImageViewCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub image: VkImage,
    pub viewType: u32,
    pub format: u32,
    pub components: VkComponentMapping,
    pub subresourceRange: VkImageSubresourceRange,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkBufferCopy {
    pub srcOffset: VkDeviceSize,
    pub dstOffset: VkDeviceSize,
    pub size: VkDeviceSize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkBufferImageCopy {
    pub bufferOffset: VkDeviceSize,
    pub bufferRowLength: u32,
    pub bufferImageHeight: u32,
    pub imageSubresource: VkImageSubresourceLayers,
    pub imageOffset: VkOffset3D,
    pub imageExtent: VkExtent3D,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkImageBlit {
    pub srcSubresource: VkImageSubresourceLayers,
    pub srcOffsets: [VkOffset3D; 2],
    pub dstSubresource: VkImageSubresourceLayers,
    pub dstOffsets: [VkOffset3D; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkViewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub minDepth: f32,
    pub maxDepth: f32,
}

#[repr(C)]
pub(crate) struct VkShaderModuleCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub codeSize: usize,
    pub pCode: *const u32,
}

#[repr(C)]
pub(crate) struct VkPipelineShaderStageCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub stage: u32,
    pub module: VkShaderModule,
    pub pName: *const c_char,
    pub pSpecializationInfo: *const c_void,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkVertexInputBindingDescription {
    pub binding: u32,
    pub stride: u32,
    pub inputRate: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkVertexInputAttributeDescription {
    pub location: u32,
    pub binding: u32,
    pub format: u32,
    pub offset: u32,
}

#[repr(C)]
pub(crate) struct VkPipelineVertexInputStateCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub vertexBindingDescriptionCount: u32,
    pub pVertexBindingDescriptions: *const VkVertexInputBindingDescription,
    pub vertexAttributeDescriptionCount: u32,
    pub pVertexAttributeDescriptions: *const VkVertexInputAttributeDescription,
}

#[repr(C)]
pub(crate) struct VkPipelineInputAssemblyStateCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub topology: u32,
    pub primitiveRestartEnable: VkBool32,
}

#[repr(C)]
pub(crate) struct VkPipelineViewportStateCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub viewportCount: u32,
    pub pViewports: *const VkViewport,
    pub scissorCount: u32,
    pub pScissors: *const VkRect2D,
}

#[repr(C)]
pub(crate) struct VkPipelineRasterizationStateCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub depthClampEnable: VkBool32,
    pub rasterizerDiscardEnable: VkBool32,
    pub polygonMode: u32,
    pub cullMode: VkFlags,
    pub frontFace: u32,
    pub depthBiasEnable: VkBool32,
    pub depthBiasConstantFactor: f32,
    pub depthBiasClamp: f32,
    pub depthBiasSlopeFactor: f32,
    pub lineWidth: f32,
}

#[repr(C)]
pub(crate) struct VkPipelineMultisampleStateCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub rasterizationSamples: u32,
    pub sampleShadingEnable: VkBool32,
    pub minSampleShading: f32,
    pub pSampleMask: *const u32,
    pub alphaToCoverageEnable: VkBool32,
    pub alphaToOneEnable: VkBool32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VkPipelineColorBlendAttachmentState {
    pub blendEnable: VkBool32,
    pub srcColorBlendFactor: u32,
    pub dstColorBlendFactor: u32,
    pub colorBlendOp: u32,
    pub srcAlphaBlendFactor: u32,
    pub dstAlphaBlendFactor: u32,
    pub alphaBlendOp: u32,
    pub colorWriteMask: VkFlags,
}

#[repr(C)]
pub(crate) struct VkPipelineColorBlendStateCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub logicOpEnable: VkBool32,
    pub logicOp: u32,
    pub attachmentCount: u32,
    pub pAttachments: *const VkPipelineColorBlendAttachmentState,
    pub blendConstants: [f32; 4],
}

#[repr(C)]
pub(crate) struct VkPipelineDynamicStateCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub dynamicStateCount: u32,
    pub pDynamicStates: *const u32,
}

#[repr(C)]
pub(crate) struct VkPipelineRenderingCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub viewMask: u32,
    pub colorAttachmentCount: u32,
    pub pColorAttachmentFormats: *const u32,
    pub depthAttachmentFormat: u32,
    pub stencilAttachmentFormat: u32,
}

#[repr(C)]
pub(crate) struct VkGraphicsPipelineCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub stageCount: u32,
    pub pStages: *const VkPipelineShaderStageCreateInfo,
    pub pVertexInputState: *const VkPipelineVertexInputStateCreateInfo,
    pub pInputAssemblyState: *const VkPipelineInputAssemblyStateCreateInfo,
    pub pTessellationState: *const c_void,
    pub pViewportState: *const VkPipelineViewportStateCreateInfo,
    pub pRasterizationState: *const VkPipelineRasterizationStateCreateInfo,
    pub pMultisampleState: *const VkPipelineMultisampleStateCreateInfo,
    pub pDepthStencilState: *const c_void,
    pub pColorBlendState: *const VkPipelineColorBlendStateCreateInfo,
    pub pDynamicState: *const VkPipelineDynamicStateCreateInfo,
    pub layout: VkPipelineLayout,
    pub renderPass: u64,
    pub subpass: u32,
    pub basePipelineHandle: VkPipeline,
    pub basePipelineIndex: i32,
}

#[repr(C)]
pub(crate) struct VkPipelineLayoutCreateInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub setLayoutCount: u32,
    pub pSetLayouts: *const u64,
    pub pushConstantRangeCount: u32,
    pub pPushConstantRanges: *const c_void,
}

#[repr(C)]
pub(crate) struct VkRenderingAttachmentInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub imageView: VkImageView,
    pub imageLayout: u32,
    pub resolveMode: u32,
    pub resolveImageView: VkImageView,
    pub resolveImageLayout: u32,
    pub loadOp: u32,
    pub storeOp: u32,
    pub clearValue: VkClearValue,
}

#[repr(C)]
pub(crate) struct VkRenderingInfo {
    pub sType: u32,
    pub pNext: *const c_void,
    pub flags: VkFlags,
    pub renderArea: VkRect2D,
    pub layerCount: u32,
    pub viewMask: u32,
    pub colorAttachmentCount: u32,
    pub pColorAttachments: *const VkRenderingAttachmentInfo,
    pub pDepthAttachment: *const VkRenderingAttachmentInfo,
    pub pStencilAttachment: *const VkRenderingAttachmentInfo,
}

// ── Chargeur ───────────────────────────────────────────────────────────────

type PfnGetInstanceProcAddr = unsafe extern "C" fn(VkInstance, *const c_char) -> *mut c_void;
type PfnGetDeviceProcAddr = unsafe extern "C" fn(VkDevice, *const c_char) -> *mut c_void;

/// Point d'entrée du chargeur Vulkan, ou `None` s'il n'est pas installé.
pub(crate) struct VkLoader {
    pub get_instance_proc_addr: PfnGetInstanceProcAddr,
}

unsafe impl Send for VkLoader {}
unsafe impl Sync for VkLoader {}

/// Ouvre `libvulkan.so.1` une seule fois. `WINLUX_NO_VULKAN=1` simule une
/// machine sans Vulkan (tests du repli).
pub(crate) fn loader() -> Option<&'static VkLoader> {
    static L: OnceLock<Option<VkLoader>> = OnceLock::new();
    L.get_or_init(|| unsafe {
        if std::env::var_os("WINLUX_NO_VULKAN").is_some_and(|v| v == "1") {
            crate::log_info!("vulkan", "Vulkan désactivé par WINLUX_NO_VULKAN");
            return None;
        }
        let mut lib = libc::dlopen(
            c"libvulkan.so.1".as_ptr(),
            libc::RTLD_NOW | libc::RTLD_LOCAL,
        );
        if lib.is_null() {
            lib = libc::dlopen(c"libvulkan.so".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
        }
        if lib.is_null() {
            crate::log_info!("vulkan", "chargeur Vulkan absent, repli OpenGL");
            return None;
        }
        let sym = libc::dlsym(lib, c"vkGetInstanceProcAddr".as_ptr());
        if sym.is_null() {
            crate::log_warn!("vulkan", "vkGetInstanceProcAddr introuvable");
            return None;
        }
        Some(VkLoader {
            get_instance_proc_addr: std::mem::transmute::<*mut c_void, PfnGetInstanceProcAddr>(sym),
        })
    })
    .as_ref()
}

/// Déclare une table de fonctions Vulkan. Le chargement échoue (`None`) si
/// un seul symbole requis manque : pas de pointeur nul appelé plus tard.
macro_rules! vk_table {
    ($table:ident, $handle:ty {
        $( $field:ident : $sym:literal => fn($($arg:ty),*) $(-> $ret:ty)? ),* $(,)?
    }) => {
        pub(crate) struct $table {
            $( pub $field: unsafe extern "C" fn($($arg),*) $(-> $ret)?, )*
        }
        unsafe impl Send for $table {}
        unsafe impl Sync for $table {}
        impl $table {
            /// # Safety
            /// `get` doit être une fonction de résolution Vulkan valide pour `handle`.
            pub unsafe fn load(
                handle: $handle,
                get: &dyn Fn($handle, *const c_char) -> *mut c_void,
            ) -> Option<Self> {
                Some(Self {
                    $( $field: {
                        let p = get(handle, concat!($sym, "\0").as_ptr().cast());
                        if p.is_null() {
                            crate::log_debug!("vulkan", "fonction absente : {}", $sym);
                            return None;
                        }
                        std::mem::transmute::<*mut c_void, unsafe extern "C" fn($($arg),*) $(-> $ret)?>(p)
                    }, )*
                })
            }
        }
    };
}

vk_table!(VkGlobalFns, VkInstance {
    create_instance: "vkCreateInstance" => fn(*const VkInstanceCreateInfo, *const c_void, *mut VkInstance) -> VkResult,
    enumerate_instance_extension_properties: "vkEnumerateInstanceExtensionProperties" => fn(*const c_char, *mut u32, *mut VkExtensionProperties) -> VkResult,
});

vk_table!(VkInstanceFns, VkInstance {
    destroy_instance: "vkDestroyInstance" => fn(VkInstance, *const c_void),
    enumerate_physical_devices: "vkEnumeratePhysicalDevices" => fn(VkInstance, *mut u32, *mut VkPhysicalDevice) -> VkResult,
    get_physical_device_properties: "vkGetPhysicalDeviceProperties" => fn(VkPhysicalDevice, *mut VkPhysicalDeviceProperties),
    get_physical_device_queue_family_properties: "vkGetPhysicalDeviceQueueFamilyProperties" => fn(VkPhysicalDevice, *mut u32, *mut VkQueueFamilyProperties),
    get_physical_device_memory_properties: "vkGetPhysicalDeviceMemoryProperties" => fn(VkPhysicalDevice, *mut VkPhysicalDeviceMemoryProperties),
    enumerate_device_extension_properties: "vkEnumerateDeviceExtensionProperties" => fn(VkPhysicalDevice, *const c_char, *mut u32, *mut VkExtensionProperties) -> VkResult,
    create_device: "vkCreateDevice" => fn(VkPhysicalDevice, *const VkDeviceCreateInfo, *const c_void, *mut VkDevice) -> VkResult,
    get_device_proc_addr: "vkGetDeviceProcAddr" => fn(VkDevice, *const c_char) -> *mut c_void,
});

vk_table!(VkSurfaceFns, VkInstance {
    destroy_surface: "vkDestroySurfaceKHR" => fn(VkInstance, VkSurfaceKHR, *const c_void),
    get_surface_support: "vkGetPhysicalDeviceSurfaceSupportKHR" => fn(VkPhysicalDevice, u32, VkSurfaceKHR, *mut VkBool32) -> VkResult,
    get_surface_capabilities: "vkGetPhysicalDeviceSurfaceCapabilitiesKHR" => fn(VkPhysicalDevice, VkSurfaceKHR, *mut VkSurfaceCapabilitiesKHR) -> VkResult,
    get_surface_formats: "vkGetPhysicalDeviceSurfaceFormatsKHR" => fn(VkPhysicalDevice, VkSurfaceKHR, *mut u32, *mut VkSurfaceFormatKHR) -> VkResult,
    get_surface_present_modes: "vkGetPhysicalDeviceSurfacePresentModesKHR" => fn(VkPhysicalDevice, VkSurfaceKHR, *mut u32, *mut u32) -> VkResult,
    create_xlib_surface: "vkCreateXlibSurfaceKHR" => fn(VkInstance, *const VkXlibSurfaceCreateInfoKHR, *const c_void, *mut VkSurfaceKHR) -> VkResult,
});

vk_table!(VkDeviceFns, VkDevice {
    destroy_device: "vkDestroyDevice" => fn(VkDevice, *const c_void),
    get_device_queue: "vkGetDeviceQueue" => fn(VkDevice, u32, u32, *mut VkQueue),
    device_wait_idle: "vkDeviceWaitIdle" => fn(VkDevice) -> VkResult,
    queue_submit: "vkQueueSubmit" => fn(VkQueue, u32, *const VkSubmitInfo, VkFence) -> VkResult,
    queue_wait_idle: "vkQueueWaitIdle" => fn(VkQueue) -> VkResult,
    create_command_pool: "vkCreateCommandPool" => fn(VkDevice, *const VkCommandPoolCreateInfo, *const c_void, *mut VkCommandPool) -> VkResult,
    destroy_command_pool: "vkDestroyCommandPool" => fn(VkDevice, VkCommandPool, *const c_void),
    allocate_command_buffers: "vkAllocateCommandBuffers" => fn(VkDevice, *const VkCommandBufferAllocateInfo, *mut VkCommandBuffer) -> VkResult,
    free_command_buffers: "vkFreeCommandBuffers" => fn(VkDevice, VkCommandPool, u32, *const VkCommandBuffer),
    begin_command_buffer: "vkBeginCommandBuffer" => fn(VkCommandBuffer, *const VkCommandBufferBeginInfo) -> VkResult,
    end_command_buffer: "vkEndCommandBuffer" => fn(VkCommandBuffer) -> VkResult,
    reset_command_buffer: "vkResetCommandBuffer" => fn(VkCommandBuffer, VkFlags) -> VkResult,
    create_fence: "vkCreateFence" => fn(VkDevice, *const VkFenceCreateInfo, *const c_void, *mut VkFence) -> VkResult,
    destroy_fence: "vkDestroyFence" => fn(VkDevice, VkFence, *const c_void),
    wait_for_fences: "vkWaitForFences" => fn(VkDevice, u32, *const VkFence, VkBool32, u64) -> VkResult,
    reset_fences: "vkResetFences" => fn(VkDevice, u32, *const VkFence) -> VkResult,
    create_semaphore: "vkCreateSemaphore" => fn(VkDevice, *const VkSemaphoreCreateInfo, *const c_void, *mut VkSemaphore) -> VkResult,
    destroy_semaphore: "vkDestroySemaphore" => fn(VkDevice, VkSemaphore, *const c_void),
    create_buffer: "vkCreateBuffer" => fn(VkDevice, *const VkBufferCreateInfo, *const c_void, *mut VkBuffer) -> VkResult,
    destroy_buffer: "vkDestroyBuffer" => fn(VkDevice, VkBuffer, *const c_void),
    get_buffer_memory_requirements: "vkGetBufferMemoryRequirements" => fn(VkDevice, VkBuffer, *mut VkMemoryRequirements),
    bind_buffer_memory: "vkBindBufferMemory" => fn(VkDevice, VkBuffer, VkDeviceMemory, VkDeviceSize) -> VkResult,
    create_image: "vkCreateImage" => fn(VkDevice, *const VkImageCreateInfo, *const c_void, *mut VkImage) -> VkResult,
    destroy_image: "vkDestroyImage" => fn(VkDevice, VkImage, *const c_void),
    get_image_memory_requirements: "vkGetImageMemoryRequirements" => fn(VkDevice, VkImage, *mut VkMemoryRequirements),
    bind_image_memory: "vkBindImageMemory" => fn(VkDevice, VkImage, VkDeviceMemory, VkDeviceSize) -> VkResult,
    create_image_view: "vkCreateImageView" => fn(VkDevice, *const VkImageViewCreateInfo, *const c_void, *mut VkImageView) -> VkResult,
    destroy_image_view: "vkDestroyImageView" => fn(VkDevice, VkImageView, *const c_void),
    allocate_memory: "vkAllocateMemory" => fn(VkDevice, *const VkMemoryAllocateInfo, *const c_void, *mut VkDeviceMemory) -> VkResult,
    free_memory: "vkFreeMemory" => fn(VkDevice, VkDeviceMemory, *const c_void),
    map_memory: "vkMapMemory" => fn(VkDevice, VkDeviceMemory, VkDeviceSize, VkDeviceSize, VkFlags, *mut *mut c_void) -> VkResult,
    unmap_memory: "vkUnmapMemory" => fn(VkDevice, VkDeviceMemory),
    create_shader_module: "vkCreateShaderModule" => fn(VkDevice, *const VkShaderModuleCreateInfo, *const c_void, *mut VkShaderModule) -> VkResult,
    destroy_shader_module: "vkDestroyShaderModule" => fn(VkDevice, VkShaderModule, *const c_void),
    create_pipeline_layout: "vkCreatePipelineLayout" => fn(VkDevice, *const VkPipelineLayoutCreateInfo, *const c_void, *mut VkPipelineLayout) -> VkResult,
    destroy_pipeline_layout: "vkDestroyPipelineLayout" => fn(VkDevice, VkPipelineLayout, *const c_void),
    create_graphics_pipelines: "vkCreateGraphicsPipelines" => fn(VkDevice, u64, u32, *const VkGraphicsPipelineCreateInfo, *const c_void, *mut VkPipeline) -> VkResult,
    destroy_pipeline: "vkDestroyPipeline" => fn(VkDevice, VkPipeline, *const c_void),
    cmd_pipeline_barrier: "vkCmdPipelineBarrier" => fn(VkCommandBuffer, VkFlags, VkFlags, VkFlags, u32, *const VkMemoryBarrier, u32, *const c_void, u32, *const VkImageMemoryBarrier),
    cmd_clear_color_image: "vkCmdClearColorImage" => fn(VkCommandBuffer, VkImage, u32, *const VkClearColorValue, u32, *const VkImageSubresourceRange),
    cmd_clear_depth_stencil_image: "vkCmdClearDepthStencilImage" => fn(VkCommandBuffer, VkImage, u32, *const VkClearDepthStencilValue, u32, *const VkImageSubresourceRange),
    cmd_copy_buffer: "vkCmdCopyBuffer" => fn(VkCommandBuffer, VkBuffer, VkBuffer, u32, *const VkBufferCopy),
    cmd_copy_buffer_to_image: "vkCmdCopyBufferToImage" => fn(VkCommandBuffer, VkBuffer, VkImage, u32, u32, *const VkBufferImageCopy),
    cmd_copy_image_to_buffer: "vkCmdCopyImageToBuffer" => fn(VkCommandBuffer, VkImage, u32, VkBuffer, u32, *const VkBufferImageCopy),
    cmd_blit_image: "vkCmdBlitImage" => fn(VkCommandBuffer, VkImage, u32, VkImage, u32, u32, *const VkImageBlit, u32),
    cmd_bind_pipeline: "vkCmdBindPipeline" => fn(VkCommandBuffer, u32, VkPipeline),
    cmd_set_viewport: "vkCmdSetViewport" => fn(VkCommandBuffer, u32, u32, *const VkViewport),
    cmd_set_scissor: "vkCmdSetScissor" => fn(VkCommandBuffer, u32, u32, *const VkRect2D),
    cmd_bind_index_buffer: "vkCmdBindIndexBuffer" => fn(VkCommandBuffer, VkBuffer, VkDeviceSize, u32),
    cmd_draw: "vkCmdDraw" => fn(VkCommandBuffer, u32, u32, u32, u32),
    cmd_draw_indexed: "vkCmdDrawIndexed" => fn(VkCommandBuffer, u32, u32, u32, i32, u32),
});

// Fonctions Vulkan 1.3 (rendu dynamique, état dynamique étendu). Absentes
// d'un pilote 1.2 : le backend refuse alors de démarrer et OpenGL prend le relais.
vk_table!(VkDevice13Fns, VkDevice {
    cmd_begin_rendering: "vkCmdBeginRendering" => fn(VkCommandBuffer, *const VkRenderingInfo),
    cmd_end_rendering: "vkCmdEndRendering" => fn(VkCommandBuffer),
    cmd_bind_vertex_buffers2: "vkCmdBindVertexBuffers2" => fn(VkCommandBuffer, u32, u32, *const VkBuffer, *const VkDeviceSize, *const VkDeviceSize, *const VkDeviceSize),
    cmd_set_primitive_topology: "vkCmdSetPrimitiveTopology" => fn(VkCommandBuffer, u32),
});

vk_table!(VkSwapchainFns, VkDevice {
    create_swapchain: "vkCreateSwapchainKHR" => fn(VkDevice, *const VkSwapchainCreateInfoKHR, *const c_void, *mut VkSwapchainKHR) -> VkResult,
    destroy_swapchain: "vkDestroySwapchainKHR" => fn(VkDevice, VkSwapchainKHR, *const c_void),
    get_swapchain_images: "vkGetSwapchainImagesKHR" => fn(VkDevice, VkSwapchainKHR, *mut u32, *mut VkImage) -> VkResult,
    acquire_next_image: "vkAcquireNextImageKHR" => fn(VkDevice, VkSwapchainKHR, u64, VkSemaphore, VkFence, *mut u32) -> VkResult,
    queue_present: "vkQueuePresentKHR" => fn(VkQueue, *const VkPresentInfoKHR) -> VkResult,
});

/// Résout les fonctions globales (sans instance).
pub(crate) fn global_fns() -> Option<&'static VkGlobalFns> {
    static G: OnceLock<Option<VkGlobalFns>> = OnceLock::new();
    G.get_or_init(|| {
        let l = loader()?;
        let gipa = l.get_instance_proc_addr;
        unsafe { VkGlobalFns::load(std::ptr::null_mut(), &|i, n| gipa(i, n)) }
    })
    .as_ref()
}

/// `vkEnumerateInstanceVersion` est optionnelle (absente en Vulkan 1.0).
pub(crate) fn instance_version() -> u32 {
    let Some(l) = loader() else { return 0 };
    unsafe {
        let p = (l.get_instance_proc_addr)(
            std::ptr::null_mut(),
            c"vkEnumerateInstanceVersion".as_ptr(),
        );
        if p.is_null() {
            return VK_API_VERSION_1_0;
        }
        let f: unsafe extern "C" fn(*mut u32) -> VkResult = std::mem::transmute(p);
        let mut v = 0;
        if f(&mut v) == VK_SUCCESS {
            v
        } else {
            VK_API_VERSION_1_0
        }
    }
}

/// Liste des extensions d'instance proposées par le chargeur.
pub(crate) fn instance_extensions() -> Vec<String> {
    let Some(g) = global_fns() else {
        return Vec::new();
    };
    unsafe {
        let mut n = 0u32;
        if (g.enumerate_instance_extension_properties)(
            std::ptr::null(),
            &mut n,
            std::ptr::null_mut(),
        ) != VK_SUCCESS
        {
            return Vec::new();
        }
        let mut props = vec![ZERO_EXTENSION; n as usize];
        let r = (g.enumerate_instance_extension_properties)(
            std::ptr::null(),
            &mut n,
            props.as_mut_ptr(),
        );
        if r != VK_SUCCESS && r != VK_INCOMPLETE {
            return Vec::new();
        }
        props.truncate(n as usize);
        props.iter().map(|p| p.name()).collect()
    }
}

/// Liste des extensions d'un périphérique physique.
///
/// # Safety
/// `inst` doit avoir été chargée pour l'instance qui possède `pd`.
pub(crate) unsafe fn device_extensions(inst: &VkInstanceFns, pd: VkPhysicalDevice) -> Vec<String> {
    let mut n = 0u32;
    if (inst.enumerate_device_extension_properties)(
        pd,
        std::ptr::null(),
        &mut n,
        std::ptr::null_mut(),
    ) != VK_SUCCESS
    {
        return Vec::new();
    }
    let mut props = vec![ZERO_EXTENSION; n as usize];
    let r = (inst.enumerate_device_extension_properties)(
        pd,
        std::ptr::null(),
        &mut n,
        props.as_mut_ptr(),
    );
    if r != VK_SUCCESS && r != VK_INCOMPLETE {
        return Vec::new();
    }
    props.truncate(n as usize);
    props.iter().map(|p| p.name()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    // Tailles relevées avec les en-têtes officiels (vulkan_core.h 1.3.275, x86-64).
    #[test]
    fn tailles_des_structures_vulkan() {
        assert_eq!(size_of::<VkPhysicalDeviceProperties>(), 824);
        assert_eq!(size_of::<VkExtensionProperties>(), 260);
        assert_eq!(size_of::<VkQueueFamilyProperties>(), 24);
        assert_eq!(size_of::<VkPhysicalDeviceMemoryProperties>(), 520);
        assert_eq!(size_of::<VkSurfaceCapabilitiesKHR>(), 52);
        assert_eq!(size_of::<VkSwapchainCreateInfoKHR>(), 104);
        assert_eq!(size_of::<VkImageCreateInfo>(), 88);
        assert_eq!(size_of::<VkImageMemoryBarrier>(), 72);
        assert_eq!(size_of::<VkGraphicsPipelineCreateInfo>(), 144);
        assert_eq!(size_of::<VkRenderingAttachmentInfo>(), 72);
        assert_eq!(size_of::<VkRenderingInfo>(), 72);
        assert_eq!(size_of::<VkBufferImageCopy>(), 56);
        assert_eq!(size_of::<VkPipelineRasterizationStateCreateInfo>(), 64);
        assert_eq!(size_of::<VkSubmitInfo>(), 72);
    }

    #[test]
    fn decodage_version() {
        let v = vk_make_api_version(0, 1, 3, 275);
        assert_eq!(version_major(v), 1);
        assert_eq!(version_minor(v), 3);
        assert_eq!(version_patch(v), 275);
    }
}
