#![feature(asm)] // 使用asm!宏
#![feature(naked_functions)] // 启用裸函数特性
                             // rust在编译函数时会为每个函数添加一些开头和结尾
                             // 将函数标记为裸函数是为了删除开头和结尾
                             // 目的是为了避免未对其的栈 避免切换上下文时的问题
use std::ptr;

const DEFAULT_STACK_SIZE: usize = 1024 * 1024 * 2;
const MAX_THREADS: usize = 4;
static mut RUNTIME: usize = 0; // 指向运行时的指针

// 创建一个运行时 以调度，切换线程
pub struct Runtime {
    threads: Vec<Thread>, // 线程数组
    current: usize,       // 当前线程
}

// State枚举 表示线程可以处于的状态
#[derive(PartialEq, Eq, Debug)]
enum State {
    Available, // 线程可用 并在需要时可分配任务
    Running,   // 线程正在运行
    Ready,     // 线程准备好继续进展，执行
}

// Thread保存线程数据 每个线程都有一个ID 所以可以将线程分离
// 这个Thread就是我们要实现的绿色线程
// id 线程ID
// stack 一块连续内存（栈）
// Vec在调用push等方法时会重新分配内存地址 这里更好的做法是使用自定义类型
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
struct ThreadContext {
    // r 代表 register r是一种常见的多CPU架构中的前缀，其中的寄存器进行了编号
    rsp: u64, // 栈指针寄存器 其内存放着一个指针，该指针永远指向系统栈最上面一个栈帧的栈顶
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    rbx: u64,
    rbp: u64, // 基址指针寄存器，其内存放着一个指针，该指针永远指向系统栈最上面一个栈帧的底部
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
    pub fn new() -> Self {
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
        while self.t_yield() {}
        std::process::exit(0);
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
        // 遍历所有其他的线程 查看是否处于就绪状
        // 如果没有 直接返回
        while self.threads[pos].state != State::Ready {
            pos += 1;
            if pos == self.threads.len() {
                pos = 0;
            }
            if pos == self.current {
                return false;
            }
        }

        if self.threads[self.current].state != State::Available {
            self.threads[self.current].state = State::Ready;
        }

        self.threads[pos].state = State::Running;
        let old_pos = self.current;
        self.current = pos;
        unsafe {
            // 调用 switch 来保存当前上下文（旧上下文）并将新上下文加载到 CPU 中
            switch(&mut self.threads[old_pos].ctx, &self.threads[pos].ctx);
        }
        // 防止 Windows 的编译器优化我们的代码
        self.threads.len() > 0
    }

    pub fn spawn(&mut self, f: fn()) {
        // 找到可用线程
        let available = self
            .threads
            .iter_mut()
            .find(|t| t.state == State::Available)
            .expect("no available thread.");
        // 获取该栈长度
        let size = available.stack.len();
        // 获取指向字节数组的可变指针
        let s_ptr = available.stack.as_mut_ptr();
        // 设置基指针为f 并16字节对齐
        // 压入guard函数 不是16字节对齐 但f返回时cpu将读取下个地址作为f的返回值
        // 设置rsp值 指向函数地址的栈指针
        unsafe {
            ptr::write(s_ptr.offset((size - 24) as isize) as *mut u64, guard as u64);
            ptr::write(s_ptr.offset((size - 32) as isize) as *mut u64, f as u64);
            available.ctx.rsp = s_ptr.offset((size - 32) as isize) as u64;
        }
        available.state = State::Ready;
    }
}

// 该函数意味传入的函数已经返回cpu执行完f 开始执行guard
// 取消引用并调用t_return()
// t_return 标记为 Available和yield
fn guard() {
    unsafe {
        let rt_ptr = RUNTIME as *mut Runtime;
        (*rt_ptr).t_return();
    };
}

// 辅助函数
// 仅为了在代码其他地方调用yield
// 假设调用该函数 runtime未初始化 或被删除 会导致未定义
pub fn yield_thread() {
    unsafe {
        let rt_ptr = RUNTIME as *mut Runtime;
        (*rt_ptr).t_yield();
    }
}

// 内联汇编  simple分支有解释
// 读取old线程
#[naked] // 裸函数
#[inline(never)] // 阻止编译器内敛此函数 否则release模式下会运行失败
unsafe fn switch(old: *mut ThreadContext, new: *const ThreadContext) {
    // 保存和恢复执行
    // 16进制
    // 0x00 0
    // 0x08 8
    // 0x10 16
    // 因为使用了兼容c内存布局 所以我们知道数据将以这种方式在内存中表示
    // rust ABI 不保证他们在内存中以相同顺序表示 但是c ABI可以保证
    asm!("
        mov %rsp, 0x00($0)
        mov %r15, 0x08($0)
        mov %r14, 0x10($0)
        mov %r13, 0x18($0)
        mov %r12, 0x20($0)
        mov %rbx, 0x28($0)
        mov %rbp, 0x30($0)

        mov 0x00($1), %rsp
        mov 0x08($1), %r15
        mov 0x10($1), %r14
        mov 0x18($1), %r13
        mov 0x20($1), %r12
        mov 0x28($1), %rbx
        mov 0x30($1), %rbp
        ret
        "
    :
    :"r"(old), "r"(new)
    :
    : "volatile", "alignstack"
    )
}

fn main() {
    let mut runtime = Runtime::new();
    runtime.init();
    runtime.spawn(|| {
        println!("THREAD 1 STARTING");
        let id = 1;
        for i in 0..10 {
            println!("thread: {} counter: {}", id, i);
            yield_thread();
        }
        println!("THREAD 1 FINISHED");
    });
    runtime.spawn(|| {
        println!("THREAD 2 STARTING");
        let id = 2;
        for i in 0..15 {
            println!("thread: {} counter: {}", id, i);
            yield_thread();
        }
        println!("THREAD 2 FINISHED");
    });
    runtime.run();
}
