#![feature(asm)]                    // 使用asm!宏
#![feature(naked_functions)]        // 启用裸函数特性
// rust在编译函数时会为每个函数添加一些开头和结尾 
// 将函数标记为裸函数是为了删除开头和结尾 
// 目的是为了避免未对其的栈 避免切换上下文时的问题
use std::ptr;

const DEFAULT_STACK_SIZE: usize = 1024 * 1024 * 2;
const MAX_THREADS: usize = 4;
static mut RUNTIME: usize = 0;      // 指向运行时的指针

// 创建一个运行时 以调度，切换线程
pub struct Runtime {
    threads: Vec<Thread>,           // 线程数组
    current: usize,                 // 当前线程
}

// State枚举 表示线程可以处于的状态
#[derive(PartialEq, Eq, Debug)]
enum State {
    Available,                      // 线程可用 并在需要时可分配任务
    Running,                        // 线程正在运行
    Ready,                          // 线程准备好继续进展，执行
}

// Thread保存线程数据 每个线程都有一个ID 所以可以将它分离
struct Thread {
    id: usize,
    stack: Vec<u8>,
    ctx: ThreadContext,
    state: State,
}

#[derive(Debug, Default)]
#[repr(C)]
struct ThreadContext {
    rsp: u64,
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    rbx: u64,
    rbp: u64,
}

impl Thread {
    fn new(id: usize) -> Self {
        Thread {
            id,
            stack: vec![0_u8; DEFAULT_STACK_SIZE],
            ctx: ThreadContext::default(),
            state: State::Available,
        }
    }
}
