#![cfg(target_os = "windows")]
use serde::Deserialize;

thread_local! {
    static COM_LIB: wmi::COMLibrary = wmi::COMLibrary::new().unwrap();
}

#[derive(Deserialize, Debug)]
#[serde(rename = "Win32_OptionalFeature")]
#[serde(rename_all = "PascalCase")]
struct OptionalFeature {
    name: String,
    // InstallState: 1 = Enabled, 2 = Disabled, 3 = Absent
    install_state: u32,
}

pub mod wsl {
    use super::*;

    pub fn check_wsl_via_wmi() -> Result<(bool, bool), wmi::WMIError> {
        use wmi::WMIConnection;
        let com_lib = COM_LIB.with(|com| *com);
        let wmi_con = WMIConnection::new(com_lib.into())?;

        // 构建 WMI 查询
        let query = "SELECT Name, InstallState FROM Win32_OptionalFeature WHERE Name = 'Microsoft-Windows-Subsystem-Linux' OR Name = 'VirtualMachinePlatform'";

        let results: Vec<OptionalFeature> = wmi_con.raw_query(query)?;

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

    pub fn check_hyperv_via_wmi() -> Result<bool, wmi::WMIError> {
        use wmi::WMIConnection;
        let com_lib = COM_LIB.with(|com| *com);
        let wmi_con = WMIConnection::new(com_lib.into())?;

        // 构建 WMI 查询
        let query =
            "SELECT Name, InstallState FROM Win32_OptionalFeature WHERE Name = 'Microsoft-Hyper-V-All'";

        let results: Vec<OptionalFeature> = wmi_con.raw_query(query)?;

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
