//! "Eject" as in Explorer: the USB device a card sits on is removed safely, with all its drives.
//! A camera's drives (slot 1, slot 2, PMHOME) are LUNs of one USB device, so they all go together.

use crate::tr;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::*;
use windows_sys::Win32::Devices::Properties::DEVPKEY_Device_InstanceId;
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::{GUID_DEVINTERFACE_DISK, IOCTL_STORAGE_GET_DEVICE_NUMBER, STORAGE_DEVICE_NUMBER};

pub fn eject(roots: &[PathBuf]) -> io::Result<()> {
    let mut devices = Vec::new();
    for root in roots {
        let device = usb_device(root)?;
        if !devices.contains(&device) {
            devices.push(device);
        }
    }
    devices.into_iter().try_for_each(request_eject)
}

/// `F:\` -> the device node of the USB device under its disk.
fn usb_device(root: &Path) -> io::Result<u32> {
    let letter = root.to_string_lossy().chars().next().unwrap_or_default();
    let number = disk_number(&open(&format!(r"\\.\{letter}:"))?)?;
    let disk = disk_interfaces()?
        .into_iter()
        .find(|path| open(path).and_then(|f| disk_number(&f)).ok() == Some(number))
        .ok_or_else(|| io::Error::other(format!("{}: {}", tr("Диск не найден", "Disk not found"), root.display())))?;
    let mut node = 0;
    let mut parent = 0;
    // unsafe: plain CfgMgr calls with buffers that live through the call
    cr(unsafe { CM_Locate_DevNodeW(&mut node, wide(&instance_id(&disk)?).as_ptr(), CM_LOCATE_DEVNODE_NORMAL) })?;
    cr(unsafe { CM_Get_Parent(&mut parent, node, 0) })?;
    Ok(parent)
}

/// Opens a volume or a disk without read or write access: enough to ask it questions.
fn open(path: &str) -> io::Result<File> {
    OpenOptions::new().access_mode(0).share_mode(3).open(path) // FILE_SHARE_READ | FILE_SHARE_WRITE
}

fn disk_number(device: &File) -> io::Result<u32> {
    let mut n = STORAGE_DEVICE_NUMBER { DeviceType: 0, DeviceNumber: 0, PartitionNumber: 0 };
    let mut got = 0;
    let size = size_of::<STORAGE_DEVICE_NUMBER>() as u32;
    let handle = device.as_raw_handle();
    let out = (&raw mut n).cast();
    let ok = unsafe { DeviceIoControl(handle, IOCTL_STORAGE_GET_DEVICE_NUMBER, std::ptr::null(), 0, out, size, &mut got, std::ptr::null_mut()) };
    if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(n.DeviceNumber) }
}

/// Paths of all disks present, e.g. `\\?\usbstor#disk&ven_jetflash…#{53f56307-…}`.
fn disk_interfaces() -> io::Result<Vec<String>> {
    let (guid, flags) = (&GUID_DEVINTERFACE_DISK, CM_GET_DEVICE_INTERFACE_LIST_PRESENT);
    let mut len = 0;
    cr(unsafe { CM_Get_Device_Interface_List_SizeW(&mut len, guid, std::ptr::null(), flags) })?;
    let mut list = vec![0u16; len as usize];
    cr(unsafe { CM_Get_Device_Interface_ListW(guid, std::ptr::null(), list.as_mut_ptr(), len, flags) })?;
    // NUL-separated strings, ending with two NULs.
    Ok(list.split(|&c| c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy).collect())
}

/// `\\?\usbstor#disk&…` -> `USBSTOR\DISK&…`, the name CM_Locate_DevNode wants.
fn instance_id(interface: &str) -> io::Result<String> {
    let mut buf = [0u16; 512];
    let mut size = size_of_val(&buf) as u32;
    let mut kind = 0;
    let path = wide(interface);
    cr(unsafe {
        CM_Get_Device_Interface_PropertyW(path.as_ptr(), &DEVPKEY_Device_InstanceId, &mut kind, buf.as_mut_ptr().cast(), &mut size, 0)
    })?;
    Ok(String::from_utf16_lossy(&buf[..size as usize / 2]).trim_end_matches('\0').to_string())
}

fn request_eject(device: u32) -> io::Result<()> {
    let mut veto = PNP_VetoTypeUnknown;
    let mut name = [0u16; 260];
    // A veto name buffer also keeps Windows from showing its own "can't eject" dialog.
    let result = unsafe { CM_Request_Device_EjectW(device, &mut veto, name.as_mut_ptr(), name.len() as u32, 0) };
    if result == CR_SUCCESS && veto == PNP_VetoTypeUnknown {
        return Ok(());
    }
    let name = String::from_utf16_lossy(&name);
    let msg = tr("Не удалось извлечь: диск занят другой программой", "Couldn't eject: the drive is in use by another program");
    Err(io::Error::other(format!("{msg} ({}, {veto})", name.trim_end_matches('\0'))))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

fn cr(result: CONFIGRET) -> io::Result<()> {
    if result == CR_SUCCESS { Ok(()) } else { Err(io::Error::other(format!("CONFIGRET {result}"))) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Finds the device of every card plugged in now; with none there's nothing to check. Ejects nothing.
    #[test]
    fn finds_device_of_connected_cards() {
        for root in crate::cards::connected() {
            let device = usb_device(&root).unwrap();
            let mut id = [0u16; 512];
            cr(unsafe { CM_Get_Device_IDW(device, id.as_mut_ptr(), id.len() as u32, 0) }).unwrap();
            println!("{} -> {}", root.display(), String::from_utf16_lossy(&id).trim_end_matches('\0'));
        }
    }
}
