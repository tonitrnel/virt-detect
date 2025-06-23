use napi_derive::napi;
use serde::Deserialize;
use std::path::Path;

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

#[napi(object)]
pub struct SystemEncoding {
    pub ansi_code: u32,
    pub oem_code: u32,
    pub ansi_encoding: &'static str,
    pub oem_encoding: &'static str,
}

#[cfg(target_os = "windows")]
#[napi]
pub fn get_system_encoding() -> SystemEncoding {
    use windows::Win32::Globalization::{GetACP, GetOEMCP};

    let ansi_code = unsafe { GetACP() };
    let ansi_encoding = match ansi_code {
        65001 => "UTF-8",
        936 => "GBK",
        950 => "BIG5",
        1252 => "WINDOWS-1252",
        932 => "SHIFT-JIS",
        _ => "UNKNOWN",
    };
    let oem_code = unsafe { GetOEMCP() };
    let oem_encoding = match oem_code {
        65001 => "UTF-8",
        936 => "GBK",
        950 => "BIG5",
        1252 => "WINDOWS-1252",
        932 => "SHIFT-JIS",
        _ => "UNKNOWN",
    };
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
pub struct QueryResult{
    pub value: bool,
    pub messages: Vec<String>,
}

#[cfg(target_os = "windows")]
#[napi]
pub async fn is_hyperv_enabled() -> QueryResult {
    let mut result = QueryResult {
        value: false,
        messages: vec![],
    };
    result.value = check_hyperv_via_wmi().await.unwrap_or_else(|err| {
        result.messages.push(format!("通过 WMI 查询 HyperVisor Optional Feature 失败, 原因: {:?}", err));
        Path::new("C:\\Windows\\System32\\vmcompute.exe").exists()
    });
    result
}

#[cfg(target_os = "windows")]
#[napi]
pub async fn is_wsl_enabled() -> QueryResult {
    let mut result = QueryResult {
        value: false,
        messages: vec![],
    };
    result.value = check_wsl_via_wmi().await.unwrap_or_else(|err| {
        result.messages.push(format!("通过 WMI 查询 WLS Optional Feature 失败, 原因: {:?}", err));
        check_wsl_via_reg()
    });
    result
}

#[derive(Deserialize, Debug)]
#[serde(rename = "Win32_OptionalFeature")]
#[serde(rename_all = "PascalCase")]
struct OptionalFeature {
    // InstallState: 1 = Enabled, 2 = Disabled, 3 = Absent
    install_state: u32,
}

#[cfg(target_os = "windows")]
async fn check_wsl_via_wmi() -> Result<bool, String> {
    let results = tokio::task::spawn_blocking(|| {
        use wmi::{COMLibrary, WMIConnection};
        let com_lib = COMLibrary::new()?;
        let wmi_con = WMIConnection::new(com_lib.into())?;

        // 构建 WMI 查询
        let query = "SELECT InstallState FROM Win32_OptionalFeature WHERE Name = 'Microsoft-Windows-Subsystem-Linux'";

        let results: Vec<OptionalFeature> = wmi_con.raw_query(query)?;
        Ok(results) as Result<Vec<OptionalFeature>, wmi::WMIError>
    })
        .await;
    let results = match results {
        Ok(results) => results,
        Err(err) => return Err(format!("无法在新的线程执行 WMI 查询，原因: {:?}", err)),
    };
    let results = match results {
        Ok(results) => results,
        Err(err) => {
            return match err {
                wmi::WMIError::HResultError { hres } => Err(format!(
                    "WMI error: {}#{hres}",
                    windows::core::HRESULT::from_nt(hres).message()
                )),
                _ => Err(format!("WMI error: {:?}", err)),
            };
        }
    };

    if let Some(feature) = results.first() {
        // InstallState == 1 意味着 "Enabled"
        Ok(feature.install_state == 1)
    } else {
        Ok(false)
    }
}

#[cfg(target_os = "windows")]
fn check_wsl_via_reg() -> bool {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;
    RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SYSTEM\CurrentControlSet\Services\lxss").is_ok()
}

#[cfg(target_os = "windows")]
async fn check_hyperv_via_wmi() -> Result<bool, String> {
    let results = tokio::task::spawn_blocking(|| {
        use wmi::{COMLibrary, WMIConnection};
        let com_lib = COMLibrary::new()?;
        let wmi_con = WMIConnection::new(com_lib.into())?;

        // 构建 WMI 查询
        let query =
            "SELECT InstallState FROM Win32_OptionalFeature WHERE Name = 'Microsoft-Hyper-V-All'";

        let results: Vec<OptionalFeature> = wmi_con.raw_query(query)?;
        Ok(results) as Result<Vec<OptionalFeature>, wmi::WMIError>
    })
    .await;
    let results = match results {
        Ok(results) => results,
        Err(err) => return Err(format!("无法在新的线程执行 WMI 查询，原因: {:?}", err)),
    };
    let results = match results {
        Ok(results) => results,
        Err(err) => {
            return match err {
                wmi::WMIError::HResultError { hres } => Err(format!(
                    "WMI error: {}#{hres}",
                    windows::core::HRESULT::from_nt(hres).message()
                )),
                _ => Err(format!("WMI error: {:?}", err)),
            };
        }
    };

    if let Some(feature) = results.first() {
        // println!("通过 WMI 查询到功能状态: {:?}", feature);
        // InstallState == 1 意味着 "Enabled"
        Ok(feature.install_state == 1)
    } else {
        // println!("WMI 查询未返回任何关于 'Microsoft-Windows-Subsystem-Linux' 的信息。");
        Ok(false)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        get_gpu_guid()
    }
    
    #[test]
    fn test_wsl_via_reg() {
        assert!(check_wsl_via_reg());
    }
}
