#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
/// 通过 cpuid 检测是否处于 hyperv 环境下
///
/// 如果处于 hyperv 那么 `check_virtual_support` 和 `is_virtualization_enabled_in_firmware_windows` 可能无法正常工作
pub fn check_hyperv_environment_cpuid() -> (bool, bool, String) {
    use std::arch::x86_64::__cpuid;
    let cpuid_leaf_40000000 = unsafe { __cpuid(0x40000000) };
    let mut hyperv_signature_bytes = Vec::new();
    hyperv_signature_bytes.extend_from_slice(&cpuid_leaf_40000000.ebx.to_ne_bytes());
    hyperv_signature_bytes.extend_from_slice(&cpuid_leaf_40000000.ecx.to_ne_bytes());
    hyperv_signature_bytes.extend_from_slice(&cpuid_leaf_40000000.edx.to_ne_bytes());

    let hyperv_signature = String::from_utf8_lossy(&hyperv_signature_bytes)
        .trim_matches('\0')
        .to_string();
    let is_hyperv_present =
        hyperv_signature.starts_with("Microsoft Hv") || hyperv_signature.starts_with("MicrosoftXv");

    let cpuid_leaf_1 = unsafe { __cpuid(0x1) };
    let is_guest_vm = (cpuid_leaf_1.ecx & (1 << 31)) != 0;

    (is_hyperv_present, is_guest_vm, hyperv_signature)
}

/// 检查是否支持虚拟化
///
/// ！注意：该函数仅支持检测 CPU 是否支持虚拟化，但不支持检测 BIOS 是否启用了虚拟化
#[cfg(target_arch = "x86_64")]
pub fn check_virtual_support() -> (bool, String, &'static str) {
    use std::arch::x86_64::__cpuid_count;

    // 检查 Intel VT-x (VMX) 或 AMD-V (SVM)
    // EAX=1: 处理器信息和功能位
    // 首先，获取供应商 ID 以便进行针对性检查
    let cpuid_vendor = unsafe { __cpuid_count(0, 0) };
    // 将 ebx, edx, ecx 中的字符拼接起来
    let vendor_id_bytes: [u8; 12] = [
        (cpuid_vendor.ebx & 0xFF) as u8,
        ((cpuid_vendor.ebx >> 8) & 0xFF) as u8,
        ((cpuid_vendor.ebx >> 16) & 0xFF) as u8,
        ((cpuid_vendor.ebx >> 24) & 0xFF) as u8,
        (cpuid_vendor.edx & 0xFF) as u8,
        ((cpuid_vendor.edx >> 8) & 0xFF) as u8,
        ((cpuid_vendor.edx >> 16) & 0xFF) as u8,
        ((cpuid_vendor.edx >> 24) & 0xFF) as u8,
        (cpuid_vendor.ecx & 0xFF) as u8,
        ((cpuid_vendor.ecx >> 8) & 0xFF) as u8,
        ((cpuid_vendor.ecx >> 16) & 0xFF) as u8,
        ((cpuid_vendor.ecx >> 24) & 0xFF) as u8,
    ];
    let vendor_id = String::from_utf8_lossy(&vendor_id_bytes);

    if vendor_id.contains("GenuineIntel") {
        // 检查 VMX (Intel VT-x)
        // EAX=1, ECX 寄存器的第 5 位
        let cpuid_features = unsafe { __cpuid_count(1, 0) };
        let vmx_supported = (cpuid_features.ecx & (1 << 5)) != 0;
        (vmx_supported, vendor_id.to_string(), "Intel VT-x (VMX)")
    } else if vendor_id.contains("AuthenticAMD") {
        // 检查 SVM (AMD-V)
        // EAX=0x80000001, ECX 寄存器的第 2 位
        let cpuid_ext_features = unsafe { __cpuid_count(0x80000001, 0) };
        let svm_supported = (cpuid_ext_features.ecx & (1 << 2)) != 0;
        (svm_supported, vendor_id.to_string(), "AMD-V (SVM)")
    } else {
        (false, vendor_id.to_string(), "Unknown")
    }
}

#[cfg(target_arch = "aarch64")]
pub fn check_virtual_support() -> (bool, String, &'static str) {
    (false, "N/A".to_string(), "Not supported")
}

#[cfg(target_os = "linux")]
/// 检查 KVM 版本
pub fn check_kvm_via_api_linux() -> (bool, String) {
    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;
    use std::path::Path;

    const KVM_GET_API_VERSION: libc::c_ulong = 0xAE00;
    if !Path::new("/dev/kvm").exists() {
        return (false, "/dev/kvm 设备文件不存在".to_string());
    }
    match OpenOptions::new().read(true).write(true).open("/dev/kvm") {
        Ok(file) => {
            let fd = file.as_raw_fd();
            let api_version = unsafe { libc::ioctl(fd, KVM_GET_API_VERSION) };
            match api_version {
                12 => (
                    true,
                    format!(
                        "/dev/kvm 可访问且 API 版本为 {} (预期值)。KVM 已启用。",
                        api_version
                    ),
                ),
                0.. => (
                    true,
                    format!(
                        "/dev/kvm 可访问，API 版本为 {}。KVM 可能已启用。",
                        api_version
                    ),
                ),
                _ => {
                    let err_no = unsafe { *libc::__errno_location() };
                    (
                        false,
                        format!(
                            "/dev/kvm 打开成功，但 ioctl(KVM_GET_API_VERSION) 失败。错误码: {}. KVM 可能未完全启用或权限不足。",
                            err_no
                        ),
                    )
                }
            }
        }
        Err(e) => (
            false,
            format!(
                "无法打开 /dev/kvm: {}. 确保有足够权限，且 kvm 内核模块 (kvm_intel 或 kvm_amd) 已加载。",
                e
            ),
        ),
    }
}

#[cfg(target_os = "macos")]
pub fn check_hypervisor_support_macos() -> (bool, String) {
    use libc::{c_int, c_void, size_t, sysctlbyname};
    use std::ffi::CString;
    use std::mem;

    let name_c = match CString::new("kern.hv_support") {
        Ok(s) => s,
        Err(_) => return (false, "无法创建 CString 用于 sysctlbyname。".to_string()),
    };

    let mut value: c_int = 0;
    let mut size: size_t = mem::size_of::<c_int>();
    let oldp = &mut value as *mut _ as *mut c_void;
    let oldlenp = &mut size as *mut size_t;

    let ret = unsafe { sysctlbyname(name_c.as_ptr(), oldp, oldlenp, std::ptr::null_mut(), 0) };

    if ret == 0 {
        if value == 1 {
            (
                true,
                "kern.hv_support (Hypervisor Framework) 为 1，虚拟化已启用。".to_string(),
            )
        } else {
            (
                false,
                format!(
                    "kern.hv_support (Hypervisor Framework) 为 {}，虚拟化未启用或不受支持。",
                    value
                ),
            )
        }
    } else {
        let err_no = unsafe { *libc::__error() };
        (false, format!("sysctlbyname 调用失败。错误码: {}", err_no))
    }
}

#[cfg(target_os = "windows")]
pub fn check_virtualization_enabled_windows() -> (bool, String) {
    use windows::Win32::System::Threading::{
        IsProcessorFeaturePresent,
        PF_VIRT_FIRMWARE_ENABLED, // 值为 19（0x13）
    };
    // 适用于 Windows8 / Server 2012 及更高版本
    let result = unsafe { IsProcessorFeaturePresent(PF_VIRT_FIRMWARE_ENABLED) };
    if result.as_bool() {
        (true, "虚拟化已在固件中启用".to_string())
    } else {
        let (is_hyperv, _, sign) = check_hyperv_environment_cpuid();
        if is_hyperv {
            (true, "虚拟化检测在 Hypervisor 下失效".to_string())
        } else {
            (
                false,
                format!("虚拟化未在固件中启用或此检查不受支持(CPU Sign: {sign})"),
            )
        }
    }
}
