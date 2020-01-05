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

// Thread保存线程数据 每个线程都有一个ID 所以可以将线程分离
// 这个Thread就是我们要实现的绿色线程
struct Thread {
    id: usize,
    stack: Vec<u8>,
    ctx: ThreadContext,
    state: State,
}

// 4个64位通用寄存器：RAX、RBX、RCX、RDX
// 4个64位指令寄存器：RSI、RDI、RBP、RSP
#[derive(Debug, Default)]
#[repr(C)]
struct ThreadContext {  // r 代表 register r是一种常见的多CPU架构中的前缀，其中的寄存器进行了编号
    rsp: u64,           // 栈指针寄存器 其内存放着一个指针，该指针永远指向系统栈最上面一个栈帧的栈顶
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    rbx: u64,
    rbp: u64,           // 基址指针寄存器，其内存放着一个指针，该指针永远指向系统栈最上面一个栈帧的底部
}

// 新线程在available状态下启动
// stack分配了栈内存 这不是必须的 也不是资源最佳使用方法
// 我们应该在首次使用时分配 而不是为一个可能需要的线程分配内存
// 但是这降低了代码的复杂性
// 一旦分配了内存就不能移动 也不能使用数组的push() 或其他触发内存重分配的方法
// 这里更好的做法是创建自定义类型 只暴露安全的方法
// Vec<T> 有一个into_boxed_slice() 方法 返回一个堆分配的切片Box<[T]> 
// 如果改为它 可以避免重新分配问题
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

impl Runtime {
    // 初始线程，初始化为running状态
    pub fn new() -> self {
        let base_thread = Thread {
            id: 0,
            stack: vec![0_u8; DEFAULT_STACK_SIZE],
            ctx: ThreadContext::default(),
            state: State::Running,
        };
        let mut threads = vec![base_thread];
        let mut available_threads: Vec<Thread> = (1..MAX_THREADS).map(|i| Thread::new(i)).collect();
        threads.append(&mut available_threads);
        Runtime {
            threads,
            current: 0,
        }
    }

    pub fn init(&self) {
        unsafe {
            let r_ptr: *const Runtime = self;
            RUNTIME = r_ptr as usize;
        }
    }

    // 开始启动运行的地方
    // 循环调用yield 当返回false 表示没有work要执行 退出进程
    pub fn run(&mut self) -> ! {
        while self.t_yield() {
            std::process::exit(0);
        }
    }

    // 线程完成时调用的返回函数
    fn t_return(&mut self) {
        if self.current != 0 {
            self.threads[self.current].state = State::Available;
            self.t_yield();
        }
    }

    // runtime的核心
    fn t_yield(&mut self) -> bool {
        let mut pos = self.current;
        // 遍历所有线程 查看是否处于就绪状态
        while self.threads[pos].state != State::Ready {
            // 0是我们的基础线程 所以每次是从1开始循环
            pos += 1;
            // pos大于当前所有线程数量时置0 从头继续遍历
            if pos == self.threads.len() {
                pos = 0;
            }
            if pos == self.current {
                return false;
            }
        }

        // 找到一个准备运行的线程 从running改为ready
        if self.threads[self.current].state != State::Available {
            self.threads[self.current].state = State::Ready;
        }

        self.threads[pos].state = State::Running;
        let old_pos = self.current;
        self.current = pos;
        unsafe {
            switch(&mut self.threads[old_pos].ctx, &self.threads[pos].ctx);
        }
        self.threads.len() > 0
    }

    pub fn spawn(&mut self, f: fn()) {
        let available = self.threads.iter_mut().find(|t| t.state == State::Available).expect("no available thread.");
        let size = available.stack.len();
        let s_ptr = available.stack.as_mut_ptr();

        unsafe {
            ptr::write(s_ptr.offset((size - 24) as isize) as *mut u64, guard as u64);
            ptr::write(s_ptr.offset((size - 32) as isize) as *mut u64, f as u64);
            available.ctx.rsp = s_ptr.offset((size - 32) as isize) as u64;
        }
        available.state = State::Ready;
    }
}
