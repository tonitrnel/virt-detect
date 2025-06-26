#![cfg(target_os = "windows")]
use serde::{Deserialize, de::DeserializeOwned};

#[derive(Deserialize, Debug)]
#[serde(rename = "Win32_OptionalFeature")]
#[serde(rename_all = "PascalCase")]
struct OptionalFeature {
    name: String,
    // InstallState: 1 = Enabled, 2 = Disabled, 3 = Absent
    install_state: u32,
}

fn wmi_err_to_string(err: &wmi::WMIError) -> String {
    match err {
        wmi::WMIError::HResultError { hres } => {
            format!(
                "WMI 查询失败, 原因: {:?}({hres}), COM 线程状态: {:?}",
                windows::core::HRESULT::from_nt(*hres).message(),
                get_thread_com_state()
            )
        }
        _ => {
            format!("WMI 查询失败, 原因: {err:?}")
        }
    }
}

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

fn execute_wmi_query<T: DeserializeOwned + Send + 'static>(
    query: &'static str,
) -> Result<Vec<T>, String> {
    // 使用新线程来出现防止 STA、MTA 问题
    let task = std::thread::spawn(move || -> Result<Vec<T>, wmi::WMIError> {
        let com_lib = wmi::COMLibrary::new()?;
        let wmi_con = wmi::WMIConnection::new(com_lib)?;

        let results: Vec<T> = wmi_con.raw_query(query)?;
        Ok(results)
    });
    let results = task
        .join()
        .map_err(|err| format!("在新线程执行 WMI 查询失败, 原因: {err:?}"))?
        .map_err(|err| wmi_err_to_string(&err))?;

    Ok(results)
}

pub mod wsl {
    use super::*;

    pub fn check_wsl_via_wmi() -> Result<(bool, bool), String> {
        // 构建 WMI 查询
        let query = "SELECT Name, InstallState FROM Win32_OptionalFeature WHERE Name = 'Microsoft-Windows-Subsystem-Linux' OR Name = 'VirtualMachinePlatform'";

        let results: Vec<OptionalFeature> = execute_wmi_query(query)?;

        let mut wsl_enabled = false;
        let mut vmp_enabled = false;

        for feature in results {
            // InstallState = 1 表示 "Enabled"
            if feature.install_state == 1 {
                if feature.name == "Microsoft-Windows-Subsystem-Linux" {
                    wsl_enabled = true;
                } else if feature.name == "VirtualMachinePlatform" {
                    vmp_enabled = true;
                }
            }
        }

        Ok((wsl_enabled, vmp_enabled))
    }
    pub fn check_wsl_via_reg() -> bool {
        use winreg::RegKey;
        use winreg::enums::HKEY_LOCAL_MACHINE;
        RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(r"SYSTEM\CurrentControlSet\Services\lxss")
            .is_ok()
    }
    pub fn check_wsl_via_service() -> Result<bool, Box<dyn std::error::Error>> {
        use windows_service::service::{ServiceAccess, ServiceState};

        use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
        let service = manager.open_service("LxssManager", ServiceAccess::QUERY_STATUS)?;
        let status = service.query_status()?;
        Ok(status.current_state == ServiceState::Running)
    }
}

pub mod hypervisor {
    use super::*;

    pub fn check_hyperv_via_wmi() -> Result<bool, String> {
        // 构建 WMI 查询
        let query = "SELECT Name, InstallState FROM Win32_OptionalFeature WHERE Name = 'Microsoft-Hyper-V-All'";

        let results: Vec<OptionalFeature> = execute_wmi_query(query)?;

        if let Some(feature) = results.first() {
            // println!("通过 WMI 查询到功能状态: {:?}", feature);
            // InstallState == 1 意味着 "Enabled"
            Ok(feature.install_state == 1)
        } else {
            // println!("WMI 查询未返回任何关于 'Microsoft-Windows-Subsystem-Linux' 的信息。");
            Ok(false)
        }
    }

    pub fn check_hyperv_via_service() -> Result<bool, Box<dyn std::error::Error>> {
        use windows_service::service::{ServiceAccess, ServiceState};
        use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
        let service = manager.open_service("vmms", ServiceAccess::QUERY_STATUS)?;
        let status = service.query_status()?;
        Ok(status.current_state == ServiceState::Running)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wsl_via_reg() {
        assert!(wsl::check_wsl_via_reg());
    }

    #[test]
    fn test_wmi() {
        println!("{:?}", hypervisor::check_hyperv_via_wmi().unwrap());
        println!("{:?}", wsl::check_wsl_via_wmi().unwrap());
    }
    #[test]
    fn test_check_hyperv_via_service() {
        println!("{:?}", hypervisor::check_hyperv_via_service().unwrap());
    }
    #[test]
    fn test_check_wsl_via_service() {
        println!("{:?}", wsl::check_wsl_via_service().unwrap());
    }
}
