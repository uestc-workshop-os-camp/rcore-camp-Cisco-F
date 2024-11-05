//! Process management syscalls
//!
use alloc::sync::Arc;

use crate::{
    config::CLOCK_FREQ,
    fs::{open_file, OpenFlags},
    mm::{translated_refmut, translated_str, PhysAddr, VirtAddr},
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next, suspend_current_and_run_next, TaskInfo, ppn_by_vpn, 
    }, timer::get_time,
};
use core::fmt::Debug;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    //trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    //trace!("kernel: sys_waitpid");
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
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
    let time = TimeVal { sec, usec };

    let token = current_user_token();
    let ti_ref = translated_refmut(token, _ts);
    *ti_ref = time;

    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info");
    if _ti.is_null() {
        return -1;
    }

    let task_info = current_task().unwrap().inner_exclusive_access().get_task_info();
    let token = current_user_token();

    let ti_ref = translated_refmut(token, _ti);
    *ti_ref = task_info;

    0
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!("kernel: sys_mmap, start: {}, len: {}, port: {}", _start, _len, _port);
    current_task().unwrap().inner_exclusive_access().cur_task_mmap(_start, _len, _port)
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap, start: {}, len: {}", _start, _len);
    current_task().unwrap().inner_exclusive_access().cur_task_munmap(_start, _len)
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    trace!("kernel: pid[{}] sys_spawn", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, _path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let data = app_inode.read_all();
        let new_task = current_task().unwrap().spawn(data.as_slice());
        let new_pid = new_task.getpid();
        let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
        trap_cx.x[10] = 0;
        add_task(new_task);

        new_pid as isize
    } else {
        error!("kernel: sys_spawn failed!");
        -1
    }
    // let parent = current_task().unwrap();
    // let token = current_user_token();
    // let path = translated_str(token, _path);

    // if let Some(data) = get_app_data_by_name(path.as_str()) {
    //     let new_task = Arc::new(TaskControlBlock::new(data));
    //     let new_pid = new_task.pid.0;
    //     // update tasks' relationship
    //     let mut parent_inner = parent.inner_exclusive_access();
    //     parent_inner.children.push(new_task.clone());
    //     new_task.inner_exclusive_access().parent = Some(Arc::downgrade(&parent));
    //     // add task to task queue
    //     add_task(new_task);    
    //     new_pid as isize
    // } else {
    //     trace!("kernel: sys_spawn invalid file name!");
    //     -1
    // }
}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(_prio: isize) -> isize {
    trace!("kernel: setting task priority: {}", _prio);
    current_task().unwrap().inner_exclusive_access().set_priority(_prio)
}

pub fn va_to_pa(va: VirtAddr) -> Option<PhysAddr> {
    let offset = va.page_offset();
    let ppn = ppn_by_vpn(va.floor());
    match ppn {
        Some(ppn) => Some(PhysAddr::from((ppn.0 << 12) | offset)),
        _ => {
            error!("kernel: va_to_pa cannot convert va to pa!");
            None
        }
    }
}
