use napi_derive::napi;

mod core;

#[napi(object)]
pub struct VirtualizationInfo {
    pub arch: &'static str,
    pub os: &'static str,
    pub cpu_supported: bool,
    pub cpu_feature_name: &'static str,
    pub os_reported_enabled: bool,
    pub os_check_details: String,
    pub overall_status_message: String,
}

#[napi]
pub fn get_virtualization() -> VirtualizationInfo {
    let (cpu_supported, _, cpu_feature_name) = core::check_virtual_support();
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "unknown"
    };
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    };
    let (os_reported_enabled, os_check_details) = {
        #[cfg(target_os = "windows")]
        {
            core::check_virtualization_enabled_windows()
        }
        #[cfg(target_os = "macos")]
        {
            core::check_hypervisor_support_macos()
        }
        #[cfg(target_os = "linux")]
        {
            core::check_kvm_via_api_linux()
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            (
                false,
                String::from("此操作系统上的启用状态检查未实现或失败"),
            )
        }
    };

    let overall_status_message = if cpu_supported && os_reported_enabled {
        "CPU 支持虚拟化，并且似乎已在操作系统/固件中启用。".to_string()
    } else if cpu_supported && !os_reported_enabled {
        format!(
            "CPU 支持虚拟化 ({})，但操作系统报告其未启用或无法确认。详情: {}",
            cpu_feature_name, os_check_details
        )
    } else if !cpu_supported && os_reported_enabled {
        format!(
            "CPU 不支持虚拟化 ({})，但操作系统报告支持，这常见于运行在虚拟系统下或不支持检测该 CPU。详情：{}",
            cpu_feature_name, os_check_details
        )
    } else {
        format!("CPU 不支持虚拟化 ({}).", cpu_feature_name)
    };

    VirtualizationInfo {
        os,
        arch,
        cpu_supported,
        cpu_feature_name,
        os_reported_enabled,
        os_check_details,
        overall_status_message,
    }
}
