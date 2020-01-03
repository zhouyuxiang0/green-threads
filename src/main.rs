// 需要使用asm!宏
#![feature(asm)]

// 设置栈的尺寸
const SSIZE: isize = 48;

#[derive(Debug, Default)]
#[repr(C)] // 告诉编译器使用兼容C-ABI的内存布局
struct ThreadContext {
    rsp: u64,
}

fn hello() -> ! {
    println!("new stack waking up!");
    loop {}
}

unsafe fn gt_switch(new: *const ThreadContext) {
    // asm!宏 检查汇编语法  语法错误时报错
    // mov 0x00($0), %rsp 将存储在基地址为$0偏移量为0x00处的值移动到rsp寄存器
    // $0为参数占位符
    // ret 指示cpu从栈顶部弹出一个内存位置并无条件跳转到该位置
    // $0实际指new参数,将new放置在栈的顶部，劫持cpu强制弹出并跳转到new此处
    asm!("
        mov 0x00($0), %rsp 
        ret
        "
    :
    : "r"(new)
    :
    : "alignstack"
    )
}

fn main() {
    let mut ctx = ThreadContext::default();
    let mut stack = vec![0_u8; SSIZE as usize];
    let stack_ptr = stack.as_mut_ptr();

    unsafe {
        std::ptr::write(stack_ptr.offset(SSIZE - 16) as * mut u64, hello as u64);
        ctx.rsp = stack_ptr.offset(SSIZE - 16) as u64;
        gt_switch(&mut ctx)
    }
}
