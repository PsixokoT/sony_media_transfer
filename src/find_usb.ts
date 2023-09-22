// import { getDeviceList } from 'usb';

import { list } from "drivelist";
import { glob } from "glob";

type SearchResult = { cwd: string, files: string[] }

export async function findUSBDrives(patterns: string[], ignore?: string[]) {
    const drives = (await list()).filter(drive => { return drive.isUSB && drive.isRemovable && Number(drive.size) > 0 });
    const result = Promise.all(drives.map(drive => drive.mountpoints[0].path).map(async cwd => {
        const files = await glob(patterns, { cwd, ignore });
        return { cwd, files };
    }));
    return (await result).filter(r => r.files.length > 0);
}