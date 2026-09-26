use std::io;

use raw_struct::FromMemoryView;

use handle::AppHandle;

#[cfg(target_os = "windows")]
use windows_sys::Win32::System::Memory::{
    VirtualAlloc,
    MEM_COMMIT,
    PAGE_EXECUTE_READWRITE,
};

#[cfg(target_os = "windows")]
type XenuineDecrypt = unsafe extern "fastcall" fn(u64, u64) -> u64;

#[cfg(target_os = "linux")]
type XenuineDecrypt = unsafe extern "win64" fn(u64, u64) -> u64;

/// Default RVA to the indirection that holds the XenuineDecrypt function pointer.
/// Adjust as needed for your target.
pub const DECRYPT_OFFSET: u64 = 0x0F37D628; // XenuineDecrypt

pub struct Decryptor {
    decrypt_key: u64,
    xenuine_decrypt_fn: XenuineDecrypt,
}

#[cfg(target_os = "linux")]
fn allocate_executable_memory(size: usize) -> Result<*mut u8, io::Error> {
    use std::ptr;
    use libc::{mmap, MAP_ANONYMOUS, MAP_PRIVATE, PROT_EXEC, PROT_READ, PROT_WRITE};
    unsafe {
        let addr = mmap(
            ptr::null_mut(),
            size,
            PROT_READ | PROT_WRITE | PROT_EXEC,
            MAP_PRIVATE | MAP_ANONYMOUS,
            -1,
            0,
        );
        if addr == libc::MAP_FAILED {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Failed to allocate executable memory with mmap: {}",
                    std::io::Error::last_os_error()
                ),
            ));
        }
        Ok(addr as *mut u8)
    }
}

#[cfg(target_os = "windows")]
fn allocate_executable_memory(size: usize) -> Result<*mut u8, io::Error> {
    unsafe {
        let addr = VirtualAlloc(std::ptr::null_mut(), size, MEM_COMMIT, PAGE_EXECUTE_READWRITE);
        if addr.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to allocate executable memory with VirtualAlloc",
            ));
        }
        Ok(addr as *mut u8)
    }
}

impl Decryptor {
    /// Builds a callable decrypt function and derives the decrypt key from the target's code.
    ///
    /// module_name: Name of the module that contains the indirection at DECRYPT_OFFSET
    pub fn create(handle: &AppHandle, module_name: &str) -> anyhow::Result<Self> {
        unsafe {
            // Read the pointer to the decrypt function from module_base + DECRYPT_OFFSET
            let ptr_addr = handle.memory_address(module_name, DECRYPT_OFFSET)?;
            let decrypt_ptr = u64::read_object(&*handle.create_memory_view(), ptr_addr)
                .map_err(|err| anyhow::anyhow!("{}", err))? as u64;

            // Derive decrypt key from instruction stream
            let tmp1_add = i32::read_object(&*handle.create_memory_view(), decrypt_ptr + 3)
                .map_err(|err| anyhow::anyhow!("{}", err))? as u64;
            let decrypt_key = tmp1_add + decrypt_ptr + 7;

            // Mirror the target function's bytes into an executable local buffer
            let mut code_buff: [u8; 1024] = [0; 1024];
            code_buff[0] = 0x90;
            code_buff[1] = 0x90;

            // Read original code after our 2-byte stub space
            handle
                .read_slice(decrypt_ptr, &mut code_buff[2..])
                .map_err(|source| anyhow::anyhow!("read code at {decrypt_ptr:#x} failed: {source}"))?;

            // Patch prologue to use RCX as the first argument (Windows fastcall)
            code_buff[2] = 0x48;
            code_buff[3] = 0x8B;
            code_buff[4] = 0xC1; // mov rax, rcx
            code_buff[5] = 0x90;
            code_buff[6] = 0x90;
            code_buff[7] = 0x90;
            code_buff[8] = 0x90;

            let executable_memory = allocate_executable_memory(code_buff.len() + 4)
                .map_err(|e| anyhow::anyhow!(
                    "Failed to allocate executable memory for XenuineDecrypt function: {}",
                    e
                ))?;

            std::ptr::copy_nonoverlapping(code_buff.as_ptr(), executable_memory, code_buff.len());

            let xenuine_decrypt_fn: XenuineDecrypt = std::mem::transmute(executable_memory);

            Ok(Self {
                decrypt_key,
                xenuine_decrypt_fn,
            })
        }
    }

    #[inline]
    pub unsafe fn decrypt(&self, a: u64) -> u64 {
        (self.xenuine_decrypt_fn)(self.decrypt_key, a)
    }
}


