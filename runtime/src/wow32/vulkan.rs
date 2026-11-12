//! vulkan-1.dll des programmes x86 (#1951).
//!
//! Le pont Vulkan de Winlux (`dlls/vulkan1.rs`) ne sert que les programmes
//! 64 bits : ses structures suivent la disposition x86-64 et la machine n'a
//! pas de pilote Vulkan 32 bits. Un programme x86 voit donc ce que voit,
//! sous Windows, un programme 32 bits sur une machine sans pilote Vulkan
//! 32 bits : le chargeur répond, l'instance est refusée par
//! VK_ERROR_INCOMPATIBLE_DRIVER, et rien ne plante. C'est ce que sondent
//! vulkandriverquery.exe de Steam et les lanceurs de jeux.
//!
//! Conventions : VKAPI_CALL vaut __stdcall en Win32 (vk_platform.h du
//! Vulkan-Headers, licence Apache 2.0 / MIT), d'où le `ret N` de chaque
//! passerelle.

use super::crt::{Fix, Override};
use super::wr;

const VK_SUCCESS: u32 = 0;
const VK_ERROR_INITIALIZATION_FAILED: u32 = -3i32 as u32;
const VK_ERROR_INCOMPATIBLE_DRIVER: u32 = -9i32 as u32;
/// VK_MAKE_API_VERSION(0, 1, 0, 0) : version du chargeur annoncée.
const VK_API_VERSION_1_0: u32 = 1 << 22;

/// Commandes globales servies par `vkGetInstanceProcAddr(NULL, …)`.
const GLOBALS: &[&str] = &[
    "vkCreateInstance",
    "vkEnumerateInstanceExtensionProperties",
    "vkEnumerateInstanceLayerProperties",
    "vkEnumerateInstanceVersion",
    "vkGetInstanceProcAddr",
];

extern "win64" fn create_instance(_info: u32, _alloc: u32, out: u32) -> u32 {
    if out != 0 {
        wr(out, 0);
    }
    crate::log_info!(
        "vulkan-1",
        "programme 32 bits : pas de pilote Vulkan 32 bits, instance refusée (VK_ERROR_INCOMPATIBLE_DRIVER)"
    );
    VK_ERROR_INCOMPATIBLE_DRIVER
}

/// Aucune extension ni couche : `*pCount = 0`.
fn empty_list(count: u32) -> u32 {
    if count == 0 {
        return VK_ERROR_INITIALIZATION_FAILED;
    }
    wr(count, 0);
    VK_SUCCESS
}

extern "win64" fn enumerate_extensions(_layer: u32, count: u32, _props: u32) -> u32 {
    empty_list(count)
}

extern "win64" fn enumerate_layers(count: u32, _props: u32) -> u32 {
    empty_list(count)
}

extern "win64" fn enumerate_version(out: u32) -> u32 {
    if out == 0 {
        return VK_ERROR_INITIALIZATION_FAILED;
    }
    wr(out, VK_API_VERSION_1_0);
    VK_SUCCESS
}

/// Seules les commandes globales existent, et seulement sans instance
/// (aucune instance ne peut être créée).
extern "win64" fn get_instance_proc_addr(instance: u32, name: u32) -> u32 {
    if name == 0 {
        return 0;
    }
    let n = unsafe { crate::dlls::cstr(name as usize as *const u8) };
    if !GLOBALS.contains(&n.as_str()) {
        return 0;
    }
    if instance != 0 && n != "vkGetInstanceProcAddr" {
        return 0;
    }
    super::override_stub("vulkan-1.dll", &n).unwrap_or(0) as u32
}

/// Pas de périphérique sans instance.
extern "win64" fn get_device_proc_addr(_device: u32, _name: u32) -> u32 {
    0
}

/// vkDestroyInstance(NULL, …) est permis et ne fait rien ; aucune autre
/// instance n'existe.
extern "win64" fn destroy_instance(_instance: u32, _alloc: u32) {}

pub(super) fn override_for(dll: &str, func: &str) -> Option<Override> {
    if dll != "vulkan-1" {
        return None;
    }
    let (target, sig): (usize, &'static str) = match func {
        "vkCreateInstance" => (create_instance as *const () as usize, "iii>i"),
        "vkEnumerateInstanceExtensionProperties" => {
            (enumerate_extensions as *const () as usize, "iii>i")
        }
        "vkEnumerateInstanceLayerProperties" => (enumerate_layers as *const () as usize, "ii>i"),
        "vkEnumerateInstanceVersion" => (enumerate_version as *const () as usize, "i>i"),
        "vkGetInstanceProcAddr" => (get_instance_proc_addr as *const () as usize, "ii>i"),
        "vkGetDeviceProcAddr" => (get_device_proc_addr as *const () as usize, "ii>i"),
        "vkDestroyInstance" => (destroy_instance as *const () as usize, "ii>v"),
        _ => return None,
    };
    Some(Override::Native {
        target,
        sig,
        fix: Fix::None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commandes_x86_servies() {
        for f in GLOBALS
            .iter()
            .chain(&["vkGetDeviceProcAddr", "vkDestroyInstance"])
        {
            assert!(override_for("vulkan-1", f).is_some(), "{f}");
        }
        assert!(override_for("vulkan-1", "vkEnumeratePhysicalDevices").is_none());
        assert!(override_for("vulkan", "vkCreateInstance").is_none());
    }
}
