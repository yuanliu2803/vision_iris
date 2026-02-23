use wgpu::hal::Api;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D12::ID3D12Device;

pub struct SharedTexture {
    pub raw_texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub shared_handle: HANDLE,
}

impl SharedTexture {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        //1、 创建纹理
        let texture_desc = wgpu::TextureDescriptor {
            label: Some("Shared_Back_Buffer"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        };
        let raw_texture = device.create_texture(&texture_desc);
        let view = raw_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let shared_handle = unsafe {
            // 1. 进入 wgpu-hal 的 DX12 设备层
            device.as_hal::<wgpu::hal::api::Dx12, _, _>(|hal_device| {
                let hal_device = hal_device.expect("Not a DX12 device");

                // 获取原生的 ID3D12Device 指针
                let raw_device: &ID3D12Device = hal_device.raw_device();

                // 2. 进入 wgpu-hal 的 DX12 纹理资源层
                raw_texture.as_hal::<wgpu::hal::api::Dx12, _, _>(|hal_texture| {
                    let hal_texture = hal_texture.expect("Not a DX12 texture");

                    // 获取原生的 ID3D12Resource
                    let raw_resource = hal_texture.raw_resource();

                    // 3. 调用 Windows 原生 API 创建共享句柄
                    let handle = raw_device
                        .CreateSharedHandle(
                            raw_resource, // 要共享的资源
                            None,         // 安全属性
                            0x10000000,   // GENERIC_ALL 访问权限
                            None,         // 名称
                        )
                        .expect("Failed to create shared handle");

                    handle
                })
            })
        };
        Self {
            raw_texture,
            view,
            shared_handle,
        }
    }
}
