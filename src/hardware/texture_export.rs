use wgpu::hal::Api;
use windows::core::Interface;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

pub struct SharedTexture {
    // WGPU 正常使用的渲染目标
    pub raw_texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub shared_handle: HANDLE,

    // 原生底层资源与复制组件
    shared_resource: ID3D12Resource,
    wgpu_resource: ID3D12Resource,
    raw_queue: ID3D12CommandQueue, // 专门为拷贝创建的独立队列
    cmd_allocator: ID3D12CommandAllocator,
    cmd_list: ID3D12GraphicsCommandList,
}

impl SharedTexture {
    // 修复点 1：去掉了参数里的 queue: &wgpu::Queue
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        // 1. 创建 WGPU 普通纹理
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

        // 2. 提取 DX12 设备和资源 (不再提取 Queue)
        let mut raw_device_opt = None;
        let mut wgpu_resource_opt = None;

        unsafe {
            device.as_hal::<wgpu::hal::api::Dx12, _, _>(|hal_device| {
                raw_device_opt = Some(hal_device.unwrap().raw_device().clone());
            });
            raw_texture.as_hal::<wgpu::hal::api::Dx12, _, _>(|hal_texture| {
                wgpu_resource_opt = Some(hal_texture.unwrap().raw_resource().clone());
            });
        }

        let raw_device = raw_device_opt.expect("Failed to extract DX12 Device");
        let wgpu_resource = wgpu_resource_opt.expect("Failed to extract DX12 Resource");

        // 3. 原生创建允许共享的 DX12 纹理
        let heap_props = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_DEFAULT,
            CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
        };

        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Alignment: 0,
            Width: width as u64,
            Height: height,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
            Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET
                | D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS,
        };

        let mut shared_resource: Option<ID3D12Resource> = None;
        unsafe {
            raw_device
                .CreateCommittedResource(
                    &heap_props,
                    D3D12_HEAP_FLAG_SHARED,
                    &desc,
                    D3D12_RESOURCE_STATE_COMMON,
                    None,
                    &mut shared_resource,
                )
                .expect("原生创建共享纹理失败");
        }
        let shared_resource = shared_resource.unwrap();

        // 4. 创建共享句柄
        let shared_handle = unsafe {
            raw_device
                .CreateSharedHandle(&shared_resource, None, 0x10000000, None)
                .expect("原生创建共享句柄失败")
        };

        // 5. 修复点 2：自己创建原生的 D3D12 命令队列
        let queue_desc = D3D12_COMMAND_QUEUE_DESC {
            Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
            Priority: 0,
            Flags: D3D12_COMMAND_QUEUE_FLAG_NONE,
            NodeMask: 0,
        };

        // 直接接收返回值，并显式声明类型为 ID3D12CommandQueue 以推断泛型 T
        let raw_queue: ID3D12CommandQueue =
            unsafe { raw_device.CreateCommandQueue(&queue_desc) }.expect("创建独立队列失败");

        let cmd_allocator: ID3D12CommandAllocator =
            unsafe { raw_device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT) }
                .expect("创建 CommandAllocator 失败");

        let cmd_list: ID3D12GraphicsCommandList = unsafe {
            raw_device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &cmd_allocator, None)
        }
        .expect("创建 CommandList 失败");

        unsafe {
            cmd_list.Close().unwrap();
        }

        Self {
            raw_texture,
            view,
            shared_handle,
            shared_resource,
            wgpu_resource,
            raw_queue,
            cmd_allocator,
            cmd_list,
        }
    }

    pub fn sync_to_shared(&self) {
        unsafe {
            self.cmd_allocator.Reset().unwrap();
            self.cmd_list.Reset(&self.cmd_allocator, None).unwrap();

            // 修复点 3：套上 `ManuallyDrop::new` 解决所有类型不匹配报错
            let transition_wgpu_to_copy = D3D12_RESOURCE_BARRIER {
                Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
                Anonymous: D3D12_RESOURCE_BARRIER_0 {
                    Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                        pResource: std::mem::ManuallyDrop::new(Some(self.wgpu_resource.clone())),
                        Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                        StateBefore: D3D12_RESOURCE_STATE_RENDER_TARGET,
                        StateAfter: D3D12_RESOURCE_STATE_COPY_SOURCE,
                    }),
                },
            };

            let transition_shared_to_dest = D3D12_RESOURCE_BARRIER {
                Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
                Anonymous: D3D12_RESOURCE_BARRIER_0 {
                    Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                        pResource: std::mem::ManuallyDrop::new(Some(self.shared_resource.clone())),
                        Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                        StateBefore: D3D12_RESOURCE_STATE_COMMON,
                        StateAfter: D3D12_RESOURCE_STATE_COPY_DEST,
                    }),
                },
            };

            let transition_wgpu_restore = D3D12_RESOURCE_BARRIER {
                Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
                Anonymous: D3D12_RESOURCE_BARRIER_0 {
                    Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                        pResource: std::mem::ManuallyDrop::new(Some(self.wgpu_resource.clone())),
                        Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                        StateBefore: D3D12_RESOURCE_STATE_COPY_SOURCE,
                        StateAfter: D3D12_RESOURCE_STATE_RENDER_TARGET,
                    }),
                },
            };

            let transition_shared_restore = D3D12_RESOURCE_BARRIER {
                Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
                Anonymous: D3D12_RESOURCE_BARRIER_0 {
                    Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                        pResource: std::mem::ManuallyDrop::new(Some(self.shared_resource.clone())),
                        Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                        StateBefore: D3D12_RESOURCE_STATE_COPY_DEST,
                        StateAfter: D3D12_RESOURCE_STATE_COMMON,
                    }),
                },
            };

            self.cmd_list
                .ResourceBarrier(&[transition_wgpu_to_copy, transition_shared_to_dest]);
            self.cmd_list
                .CopyResource(&self.shared_resource, &self.wgpu_resource);
            self.cmd_list
                .ResourceBarrier(&[transition_wgpu_restore, transition_shared_restore]);
            self.cmd_list.Close().unwrap();

            let cmd_list_cast: ID3D12CommandList = self.cmd_list.cast().unwrap();
            self.raw_queue.ExecuteCommandLists(&[Some(cmd_list_cast)]);
        }
    }
}
