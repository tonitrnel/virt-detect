use napi_derive::napi;
use std::path::Path;

mod encoding;
mod virtualization;
mod windows_feature;

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
    let (cpu_supported, _, cpu_feature_name) = virtualization::check_virtual_support();
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
            virtualization::check_virtualization_enabled_windows()
        }
        #[cfg(target_os = "macos")]
        {
            virtualization::check_hypervisor_support_macos()
        }
        #[cfg(target_os = "linux")]
        {
            virtualization::check_kvm_via_api_linux()
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

#[napi(object)]
pub struct SystemEncoding {
    pub ansi_code: u32,
    pub oem_code: u32,
    pub ansi_encoding: &'static str,
    pub oem_encoding: &'static str,
}

#[allow(deprecated)]
#[cfg(target_os = "windows")]
#[napi]
pub fn get_system_encoding() -> SystemEncoding {
    let (ansi_code, ansi_encoding) = encoding::get_system_encoding();
    let (oem_code, oem_encoding) = encoding::get_console_encoding();
    SystemEncoding {
        ansi_code,
        ansi_encoding,
        oem_code,
        oem_encoding,
    }
}

#[napi]
pub fn get_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[napi(object)]
pub struct FeatureStatus {
    pub enabled: bool,
    pub details: Vec<String>,
}

#[cfg(target_os = "windows")]
#[napi]
pub fn is_hyperv_enabled() -> FeatureStatus {
    let mut details = vec![];

    match windows_feature::hypervisor::check_hyperv_via_service() {
        Ok(running) => {
            details.push(format!(
                "服务 'vmms': 状态为 '{}'。",
                if running { "正在运行" } else { "已停止" }
            ));
            if running {
                return FeatureStatus {
                    enabled: true,
                    details,
                };
            }
        }
        Err(err) => {
            details.push(format!("服务 'vmms' 查询失败: {:?}。", err));
        }
    }
    match windows_feature::hypervisor::check_hyperv_via_wmi() {
        Ok(enabled) => {
            details.push(format!(
                "WMI 检查: Hyper-V 可选功能状态为 {}。",
                if enabled { "已启用" } else { "未启用" }
            ));
            if enabled {
                return FeatureStatus {
                    enabled: true,
                    details,
                };
            }
        }
        Err(err) => match err {
            wmi::WMIError::HResultError { hres } => {
                details.push(format!(
                    "WMI 查询失败: {:?}(#0x{:0x})",
                    windows::core::HRESULT::from_nt(hres).message(),
                    hres
                ));
            }
            _ => {
                details.push(format!("WMI 查询失败: {:?}。", err));
            }
        },
    }
    details.push("所有检测方法均未能确认 Hyper-V 已完全启用。".to_string());
    FeatureStatus {
        enabled: false,
        details,
    }
}

#[cfg(target_os = "windows")]
#[napi]
pub fn is_wsl_enabled() -> FeatureStatus {
    let mut details = vec![];

    if !Path::new("C:\\Windows\\System32\\wsl.exe").exists() {
        details.push("文件检查: 未找到 wsl.exe，WSL 未安装。".to_string());
        return FeatureStatus {
            enabled: false,
            details,
        };
    }

    details.push("文件检查: 找到 wsl.exe。".to_string());

    match windows_feature::wsl::check_wsl_via_service() {
        Ok(running) => {
            details.push(format!(
                "服务 'LxssManager': 状态为 '{}'。",
                if running { "正在运行" } else { "已停止" }
            ));
            if running {
                return FeatureStatus {
                    enabled: true,
                    details,
                };
            }
        }
        Err(err) => {
            details.push(format!("服务 'LxssManager' 查询失败: {:?}。", err));
        }
    }
    match windows_feature::wsl::check_wsl_via_reg() {
        true => {
            details.push("注册表检查: WSL 已启用。".to_string());

            return FeatureStatus {
                enabled: true,
                details,
            };
        }
        false => {
            details.push("注册表检查: WSL 未启用。".to_string());
        }
    }
    match windows_feature::wsl::check_wsl_via_wmi() {
        Ok((wsl_enabled, vmp_enabled)) => {
            details.push(format!(
                "WMI: 'Microsoft-Windows-Subsystem-Linux' 状态为 {}.",
                if wsl_enabled {
                    "已启用"
                } else {
                    "未启用"
                }
            ));
            details.push(format!(
                "WMI: 'VirtualMachinePlatform' 状态为 {}.",
                if vmp_enabled {
                    "已启用"
                } else {
                    "未启用"
                }
            ));

            let fully_enabled = wsl_enabled && vmp_enabled;
            if fully_enabled {
                return FeatureStatus {
                    enabled: true,
                    details,
                };
            }
        }
        Err(e) => {
            details.push(format!("WMI 查询可选功能失败: {:?}。", e));
        }
    }
    details.push("所有检测方法均未能确认 WSL 已完全启用。".to_string());
    FeatureStatus {
        enabled: false,
        details,
    }
}

#[cfg(target_os = "windows")]
pub fn get_gpu_guid() {
    use std::collections::HashMap;
    use wmi::{COMLibrary, Variant, WMIConnection};

    // 初始化 COM 和 WMI 连接
    let com_con = COMLibrary::new().unwrap();
    let wmi_con = WMIConnection::new(com_con.into()).unwrap();

    // 查询所有视频控制器的 PNPDeviceID
    let results: Vec<HashMap<String, Variant>> = wmi_con
        .raw_query("SELECT PNPDeviceID, CreationClassName, AdapterRAM, MaxMemorySupported, Name FROM Win32_VideoController")
        .unwrap();

    for (i, row) in results.into_iter().enumerate() {
        println!("#{i} {row:?}");
        if let Some(Variant::String(pnpid)) = row.get("PNPDeviceID") {
            println!("GPU {}: PNPDeviceID = {}", i, pnpid);
        }
    }
}

#[napi]
#[cfg(target_os = "windows")]
pub fn get_thread_com_state() -> String {
    use windows::Win32::System::Com::{APTTYPE, CoGetApartmentType};
    use windows::core::HRESULT;

    let mut apt_type = APTTYPE(0);
    let mut apt_qualifier = windows::Win32::System::Com::APTTYPEQUALIFIER(0);

    // CoGetApartmentType 是一个安全的查询函数，它不会初始化或改变任何东西
    let hr = unsafe { CoGetApartmentType(&mut apt_type, &mut apt_qualifier) };

    match hr {
        Ok(()) => match apt_type {
            windows::Win32::System::Com::APTTYPE_STA => {
                "STA (Single-Threaded Apartment)".to_string()
            }
            windows::Win32::System::Com::APTTYPE_MTA => {
                "MTA (Multi-Threaded Apartment)".to_string()
            }
            windows::Win32::System::Com::APTTYPE_NA => "NA (Neutral Apartment)".to_string(),
            _ => format!("Unknown Apartment Type ({})", apt_type.0),
        },
        Err(err) => {
            if err == HRESULT::from_win32(0x800401F0).into()
            /* CO_E_NOTINITIALIZED */
            {
                "Not Initialized".to_string()
            } else {
                format!("Failed to get apartment type, HRESULT: {:#X}", err.code().0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_guid() {
        get_gpu_guid()
    }
}
