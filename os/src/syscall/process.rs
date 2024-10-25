//! Process management syscalls
use crate::{
    config::{CLOCK_FREQ, MAX_SYSCALL_NUM}, mm::translated_byte_buffer, task::{
        change_program_brk, current_user_token, exit_current_and_run_next, suspend_current_and_run_next, TaskStatus, TASK_MANAGER
    }, timer::{get_time, get_time_us}
};
use core::slice;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    if _ts.is_null() {
        return -1;
    }

    // get current tick
    let time_ticks = get_time();
    let sec = time_ticks / CLOCK_FREQ;
    let usec = (time_ticks % CLOCK_FREQ) * 1_000_000 / CLOCK_FREQ;

    // convert the virtual address
    let token = current_user_token();
    let mut buffers = translated_byte_buffer(token, _ts as *const u8, core::mem::size_of::<TimeVal>());

    let mut buffer_offset = 0;
    let mut src_offset = 0;
    let mut src = unsafe {
        slice::from_raw_parts(
            &sec as *const _ as *const u8,
            core::mem::size_of::<usize>()
        )
    };

    let mut buffer_iter = buffers.iter_mut ();
    let mut buffer = buffer_iter.next().unwrap();

    while src_offset < src.len() {
        let len = (src.len() - src_offset).min(buffer.len() - buffer_offset);
        buffer[buffer_offset..buffer_offset + len].copy_from_slice(&src[src_offset..src_offset + len]);
        src_offset += len;
        buffer_offset += len;

        if buffer_offset == buffer.len() && src_offset == src.len() {
            break;
        }

        if buffer_offset == buffer.len() {
            buffer_offset = 0;
            buffer = buffer_iter.next().unwrap();
        }

        if src_offset == src.len() {
            src = unsafe {
                slice::from_raw_parts(
                    &usec as *const _ as *const u8,
                    core::mem::size_of::<usize>()
                )
            };
            src_offset = 0;
        }
    }

    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info NOT IMPLEMENTED YET!");
    
    if _ti.is_null() {
        return -1;
    }

    // get current task
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let cur_task_num = inner.current_task;
    let cur_task = &mut inner.tasks[cur_task_num];

    let status = cur_task.task_status;
    let syscall_times = cur_task.syscall_times;
    let time = get_time_us() - cur_task.task_start_time;

    drop(inner);

    let token = current_user_token();
    let mut buffers = translated_byte_buffer(token, _ti as *const u8, core::mem::size_of::<TaskInfo>());

    let mut buffer_iter = buffers.iter_mut();
    let mut buffer = buffer_iter.next().unwrap();
    let mut buffer_offset = 0;
    let mut src_offset = 0;
    let mut src = unsafe {
        slice::from_raw_parts(&status as *const _ as *const u8,
        core::mem::size_of::<TaskStatus>())
    };

    while src_offset < src.len() {
        let len = (src.len() - src_offset).min(buffer.len() - buffer_offset);
        buffer[buffer_offset..buffer_offset + len].copy_from_slice(&src[src_offset..src_offset + len]);
        src_offset += len;
        buffer_offset += len;

        if buffer_offset == buffer.len() && src_offset == src.len() {
            break;
        }

        // end of buffer, switch to the next page
        if buffer_offset == buffer.len() {
            buffer_offset = 0;
            buffer = buffer_iter.next().unwrap();
        }

        if src_offset == src.len() {
            if src.len() == core::mem::size_of::<TaskStatus>() {
                src = unsafe {
                    slice::from_raw_parts(&syscall_times as *const _ as *const u8,
                    core::mem::size_of::<[u32; MAX_SYSCALL_NUM]>())
                };
            } else if src.len() == core::mem::size_of::<[u32; MAX_SYSCALL_NUM]>() {
                src = unsafe {
                    slice::from_raw_parts(&time as *const _ as *const u8,
                    core::mem::size_of::<usize>())
                };
            }
            src_offset = 0;
        }
    }

    0
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    -1
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    -1
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
