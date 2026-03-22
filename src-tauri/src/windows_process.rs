#[cfg(windows)]
use std::collections::{HashMap, HashSet, VecDeque};

#[cfg(windows)]
use anyhow::Result;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE,
};
#[cfg(windows)]
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32First, Process32Next, Thread32First, Thread32Next,
    PROCESSENTRY32, TH32CS_SNAPPROCESS, TH32CS_SNAPTHREAD, THREADENTRY32,
};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    OpenThread, ResumeThread, SuspendThread, THREAD_SUSPEND_RESUME,
};

#[cfg(windows)]
struct HandleGuard(HANDLE);

#[cfg(windows)]
impl Drop for HandleGuard {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(windows)]
fn last_os_error(message: &str) -> anyhow::Error {
    let code = unsafe { GetLastError() } as i32;
    anyhow::anyhow!("{message}: {}", std::io::Error::from_raw_os_error(code))
}

#[cfg(windows)]
fn for_each_process_thread<F>(process_id: u32, mut op: F) -> Result<usize>
where
    F: FnMut(HANDLE, u32) -> Result<()>,
{
    if process_id == 0 {
        return Err(anyhow::anyhow!("Process ID is not available"));
    }

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(last_os_error("Failed to create thread snapshot"));
    }
    let _snapshot_guard = HandleGuard(snapshot);

    let mut entry = THREADENTRY32 {
        dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };

    let first = unsafe { Thread32First(snapshot, &mut entry) };
    if first == 0 {
        let code = unsafe { GetLastError() };
        if code == ERROR_NO_MORE_FILES {
            return Err(anyhow::anyhow!("No threads found for process {process_id}"));
        }

        return Err(last_os_error("Failed to enumerate process threads"));
    }

    let mut matched_threads = 0usize;
    let mut successful_ops = 0usize;
    let mut first_error: Option<anyhow::Error> = None;

    loop {
        if entry.th32OwnerProcessID == process_id {
            matched_threads += 1;

            let thread_handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if thread_handle.is_null() {
                let err = last_os_error(&format!(
                    "Failed to open thread {} for process {}",
                    entry.th32ThreadID, process_id
                ));
                log::warn!("{err}");
                if first_error.is_none() {
                    first_error = Some(err);
                }
            } else {
                let _thread_guard = HandleGuard(thread_handle);
                match op(thread_handle, entry.th32ThreadID) {
                    Ok(()) => successful_ops += 1,
                    Err(err) => {
                        log::warn!("{err}");
                        if first_error.is_none() {
                            first_error = Some(err);
                        }
                    }
                }
            }
        }

        let next = unsafe { Thread32Next(snapshot, &mut entry) };
        if next == 0 {
            let code = unsafe { GetLastError() };
            if code == ERROR_NO_MORE_FILES {
                break;
            }

            return Err(last_os_error("Failed while iterating process threads"));
        }
    }

    if matched_threads == 0 {
        return Err(anyhow::anyhow!(
            "No active threads found for process {process_id}"
        ));
    }

    if successful_ops == 0 {
        return Err(first_error.unwrap_or_else(|| {
            anyhow::anyhow!("No thread operations succeeded for process {process_id}")
        }));
    }

    Ok(successful_ops)
}

#[cfg(windows)]
fn collect_process_tree(root_process_id: u32) -> Result<Vec<u32>> {
    if root_process_id == 0 {
        return Err(anyhow::anyhow!("Process ID is not available"));
    }

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(last_os_error("Failed to create process snapshot"));
    }
    let _snapshot_guard = HandleGuard(snapshot);

    let mut entry = PROCESSENTRY32 {
        dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
        ..Default::default()
    };

    let first = unsafe { Process32First(snapshot, &mut entry) };
    if first == 0 {
        let code = unsafe { GetLastError() };
        if code == ERROR_NO_MORE_FILES {
            return Ok(vec![root_process_id]);
        }

        return Err(last_os_error("Failed to enumerate process tree"));
    }

    let mut children_by_parent: HashMap<u32, Vec<u32>> = HashMap::new();
    loop {
        children_by_parent
            .entry(entry.th32ParentProcessID)
            .or_default()
            .push(entry.th32ProcessID);

        let next = unsafe { Process32Next(snapshot, &mut entry) };
        if next == 0 {
            let code = unsafe { GetLastError() };
            if code == ERROR_NO_MORE_FILES {
                break;
            }

            return Err(last_os_error("Failed while iterating process tree"));
        }
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::from([root_process_id]);
    let mut process_ids = Vec::new();

    while let Some(process_id) = queue.pop_front() {
        if !visited.insert(process_id) {
            continue;
        }

        process_ids.push(process_id);
        if let Some(children) = children_by_parent.get(&process_id) {
            queue.extend(children.iter().copied());
        }
    }

    Ok(process_ids)
}

#[cfg(windows)]
fn for_each_process_tree_thread<F>(root_process_id: u32, mut op: F) -> Result<usize>
where
    F: FnMut(HANDLE, u32, u32) -> Result<()>,
{
    let process_ids = collect_process_tree(root_process_id)?;
    let mut total_successes = 0usize;
    let mut first_error: Option<anyhow::Error> = None;

    for process_id in process_ids {
        match for_each_process_thread(process_id, |thread_handle, thread_id| {
            op(thread_handle, process_id, thread_id)
        }) {
            Ok(successes) => total_successes += successes,
            Err(err) => {
                log::warn!("{err}");
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
    }

    if total_successes == 0 {
        return Err(first_error.unwrap_or_else(|| {
            anyhow::anyhow!("No thread operations succeeded for process tree {root_process_id}")
        }));
    }

    Ok(total_successes)
}

#[cfg(windows)]
pub fn suspend_process(root_process_id: u32) -> Result<usize> {
    for_each_process_tree_thread(root_process_id, |thread_handle, process_id, thread_id| {
        let previous_suspend_count = unsafe { SuspendThread(thread_handle) };
        if previous_suspend_count == u32::MAX {
            return Err(last_os_error(&format!(
                "Failed to suspend thread {} for process {}",
                thread_id, process_id
            )));
        }

        Ok(())
    })
}

#[cfg(windows)]
pub fn resume_process(root_process_id: u32) -> Result<usize> {
    for_each_process_tree_thread(root_process_id, |thread_handle, process_id, thread_id| {
        let previous_suspend_count = unsafe { ResumeThread(thread_handle) };
        if previous_suspend_count == u32::MAX {
            return Err(last_os_error(&format!(
                "Failed to resume thread {} for process {}",
                thread_id, process_id
            )));
        }

        Ok(())
    })
}
