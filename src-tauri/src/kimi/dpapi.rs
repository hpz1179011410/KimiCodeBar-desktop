//! Windows DPAPI 加解密（CryptProtectData / CryptUnprotectData）。
//!
//! - flags = CRYPTPROTECT_UI_FORBIDDEN（禁止弹 UI）
//! - 无 entropy，CurrentUser 作用域
//! - 输出 blob 用 LocalFree 释放
//!
//! 本应用为 Windows 专用，非 Windows 平台仅留编译边界（返回 Unsupported）。

#[derive(Debug, thiserror::Error)]
pub enum DpapiError {
    #[error("CryptProtectData 失败，错误码 {0}")]
    Protect(u32),
    #[error("CryptUnprotectData 失败，错误码 {0}")]
    Unprotect(u32),
    #[error("当前平台不支持 DPAPI")]
    Unsupported,
}

#[cfg(windows)]
pub fn protect(data: &[u8]) -> Result<Vec<u8>, DpapiError> {
    use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    unsafe {
        let input = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut output: CRYPT_INTEGER_BLOB = std::mem::zeroed();
        let ok = CryptProtectData(
            &input,
            std::ptr::null(),     // szDataDescr
            std::ptr::null(),     // pOptionalEntropy
            std::ptr::null_mut(), // pvReserved
            std::ptr::null(),     // pPromptStruct
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        );
        if ok == 0 {
            return Err(DpapiError::Protect(GetLastError()));
        }
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData.cast());
        Ok(result)
    }
}

#[cfg(windows)]
pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, DpapiError> {
    use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    unsafe {
        let input = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut output: CRYPT_INTEGER_BLOB = std::mem::zeroed();
        let ok = CryptUnprotectData(
            &input,
            std::ptr::null_mut(), // ppszDataDescr
            std::ptr::null(),     // pOptionalEntropy
            std::ptr::null_mut(), // pvReserved
            std::ptr::null(),     // pPromptStruct
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        );
        if ok == 0 {
            return Err(DpapiError::Unprotect(GetLastError()));
        }
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData.cast());
        Ok(result)
    }
}

#[cfg(not(windows))]
pub fn protect(_data: &[u8]) -> Result<Vec<u8>, DpapiError> {
    Err(DpapiError::Unsupported)
}

#[cfg(not(windows))]
pub fn unprotect(_data: &[u8]) -> Result<Vec<u8>, DpapiError> {
    Err(DpapiError::Unsupported)
}
