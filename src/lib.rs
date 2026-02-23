// moga_iris/src/lib.rs
mod hardware;

// 假设 GpuContext::new() 返回的类型可以与 IrisContext 匹配
use crate::hardware::instance::GpuContext;
use std::os::windows::raw::HANDLE;

/// 封装用于与 WPF 共享的渲染目标
pub struct SharedRenderTarget {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub shared_handle: HANDLE,
}

/// 引擎的根节点，保存所有全局状态
pub struct IrisEngine {
    pub context: GpuContext,
    pub render_target: SharedRenderTarget,
}

#[no_mangle]
pub extern "C" fn iris_create_engine() -> *mut IrisEngine {
    // 1. 初始化 GPU 上下文
    // 注意：如果 GpuContext::new() 返回的不是 IrisContext，这里需要做一下字段映射
    let context = pollster::block_on(GpuContext::new());
    // 2. 初始化共享纹理
    // 引擎启动时必须有一个初始的 RenderTarget，哪怕尺寸很小 (例如 1x1 或 800x600)
    // 这里调用你硬件模块里的逻辑来创建初始纹理。为了防止编译报错，我写了个占位函数。
    let render_target = create_initial_render_target(&context.device);

    // 3. 补全了 render_target 字段的初始化
    let engine = Box::new(IrisEngine {
        context,
        render_target,
    });

    Box::into_raw(engine)
}

#[no_mangle]
pub extern "C" fn iris_destroy_engine(engine_ptr: *mut IrisEngine) {
    if !engine_ptr.is_null() {
        unsafe {
            drop(Box::from_raw(engine_ptr));
        }
    }
}

#[no_mangle]
pub extern "C" fn iris_render_frame(engine_ptr: *mut IrisEngine) -> *mut std::ffi::c_void {
    // 增加一个空指针保护，防止 C# 端传错导致 Rust 崩溃
    if engine_ptr.is_null() {
        return std::ptr::null_mut();
    }

    let engine = unsafe { &mut *engine_ptr };
    let ctx = &engine.context;
    let shared_tex = &engine.render_target;

    // 2. 开始渲染编码
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &shared_tex.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.2,
                        b: 0.3,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        // TODO: 这里之后调用 ROI 绘制逻辑
    }

    ctx.queue.submit(Some(encoder.finish()));

    // 3. 返回 Windows 句柄给 WPF
    // 修复点：std 的 HANDLE 本身就是指针，去掉了错误的 .0
    shared_tex.shared_handle as *mut std::ffi::c_void
}

fn create_initial_render_target(device: &wgpu::Device) -> SharedRenderTarget {
    unimplemented!("请在这里调用 SharedTexture 的创建逻辑，返回 SharedRenderTarget")
}
