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
    // : 内联asm支持汇编模板语法，此处有四个附加参数，第一个为output，是传递输出参数的地方
    // : "r"(new) "r" 被称为一个 constraint（约束）。使用这些约束指导编译器决定放置输入的位置 "r" 仅表示将其放入编译器选择的通用寄存器中
    // : "alignstack" options选项，rust中的内联汇编可以设置三种选项 alignstack, volatile, intel,windows上运行需指定为 alignstack 对齐栈
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
    // 获取指向vec!切片的不安全可变指针
    let stack_ptr = stack.as_mut_ptr();
    // 64位cpu每次只能从内存中取8个字节的数据 32位是4个字节 所以64位cpu每次只能对8的倍数的地址进行读取
    // 栈向下增长，我们的48字节栈是从索引0到47 首先内存地址必须要16字节对齐 所以索引32将是从栈末尾开始的16字节偏移量的第一个索引
    unsafe {
        // std::ptr::write 使用给定的值覆盖内存位置 而不读取或删除旧值
        // hello 已经是一个函数指针 64位系统的指针都是64位的 所以直接转为u64
        std::ptr::write(stack_ptr.offset(SSIZE - 16) as * mut u64, hello as u64);
        // 将rsp栈指针设置为48-16=32的索引位置
        ctx.rsp = stack_ptr.offset(SSIZE - 16) as u64;
        // 让 CPU 跳转到我们自己的栈并在那里执行代码
        gt_switch(&mut ctx)
    }
}
