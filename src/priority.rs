// Scheduling priority for the recording process. A game that keeps the GPU
// busy otherwise delays the capture of its own frames, which shows up as
// repeated frames in the recording. Raising the GPU scheduling priority
// (what OBS does) lets capture and encoding cut in; it needs no admin
// rights but only takes effect once the process has a GPU device.
#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn CloseHandle(handle: isize) -> i32;
    fn SetPriorityClass(handle: isize, class: u32) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn D3DKMTSetProcessSchedulingPriorityClass(handle: isize, class: i32) -> i32;
}

const PROCESS_SET_INFORMATION: u32 = 0x0200;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x8000;
/// D3DKMT_SCHEDULINGPRIORITYCLASS_HIGH (REALTIME could starve the game).
const GPU_PRIORITY_HIGH: i32 = 4;

/// Raises process `pid` to high GPU and above-normal CPU priority; returns
/// whether the GPU priority was accepted.
pub fn boost(pid: u32) -> bool {
    unsafe {
        let handle = OpenProcess(PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle == 0 {
            return false;
        }
        SetPriorityClass(handle, ABOVE_NORMAL_PRIORITY_CLASS);
        let status = D3DKMTSetProcessSchedulingPriorityClass(handle, GPU_PRIORITY_HIGH);
        CloseHandle(handle);
        status == 0
    }
}
