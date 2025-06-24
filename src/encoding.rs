#[cfg(target_os = "windows")]
#[deprecated = "Powershell 使用 UTF-16le 编码，此函数无用"]
pub fn get_system_encoding() -> (u32, &'static str) {
    use windows::Win32::Globalization::GetACP;
    let ansi_code = unsafe { GetACP() };

    let ansi_encoding = match ansi_code {
        65001 => "UTF-8",
        936 => "GBK",
        950 => "BIG5",
        1252 => "WINDOWS-1252",
        932 => "SHIFT-JIS",
        _ => "UNKNOWN",
    };
    (ansi_code, ansi_encoding)
}

#[cfg(target_os = "windows")]
#[deprecated = "Powershell 使用 UTF-16le 编码，此函数无用"]
pub fn get_console_encoding() -> (u32, &'static str) {
    use windows::Win32::Globalization::GetOEMCP;
    let oem_code = unsafe { GetOEMCP() };
    let oem_encoding = match oem_code {
        65001 => "UTF-8",
        936 => "GBK",
        950 => "BIG5",
        1252 => "WINDOWS-1252",
        932 => "SHIFT-JIS",
        _ => "UNKNOWN",
    };
    (oem_code, oem_encoding)
}
