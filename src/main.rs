#![allow(unused_variables)]

#[macro_use]
extern crate vulkano;

use vulkano::instance::Instance;
use vulkano::instance::InstanceExtensions;
use vulkano::instance::PhysicalDevice;

use vulkano::device::Device;
use vulkano::device::DeviceExtensions;
use vulkano::instance::Features;

use vulkano::buffer::BufferUsage;
use vulkano::buffer::CpuAccessibleBuffer;

use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::command_buffer::CommandBuffer;
use vulkano::sync::GpuFuture;

struct MyStruct {
	a: u32,
	b: bool,
}

fn main() {
	let instance = Instance::new(None, &InstanceExtensions::none(), None).expect("failed to create instance");
	let physical = PhysicalDevice::enumerate(&instance).next().expect("no device available");
	for family in physical.queue_families() {
		println!("Found a queue family with {:?} queue(s)", family.queues_count());
	}

	let queue_family = physical.queue_families()
		.find(|&q| q.supports_graphics())
		.expect("couldn't find a graphical queue family");

	let (device, mut queues) = {
	Device::new(physical, &Features::none(), &DeviceExtensions::none(),
		[(queue_family, 0.5)].iter().cloned()).expect("failed to create device")
	};
	let queue = queues.next().unwrap();

	//tip: to figure out something's type, fail compilation on casting : 
	//device as (); //--> std::sync::Arc<vulkano::device::Device>
	//queue as (); //-> Arc<Queue>
	

	//Many kinds of buffers : CpuAccessibleBuffer, ImmutableBuffer, CpuBufferPool
	//Simplest kind of buffer is CpuAccessibleBuffer

	//you can add anything in a buffer using from_data
	let i32_data = 12;
	let buffer_from_data_1 = CpuAccessibleBuffer::from_data(device.clone(), BufferUsage::all(), i32_data).expect("failed to create buffer");

	//You can put any type you want in a buffer
	//note: using a type that doesn't implement the Send, Sync and Copy traits or that isn't 'static will restrict what you can do with that buffer.
	let struct_data = MyStruct { a: 5, b: true };
	let buffer_from_data_2 = CpuAccessibleBuffer::from_data(device.clone(), BufferUsage::all(), struct_data).unwrap();

	//use from_iter to create a buffer from any array 
	let iter = (0 .. 128).map(|_| 5u8);
	let buffer_from_iter = CpuAccessibleBuffer::from_iter(device.clone(), BufferUsage::all(), iter).unwrap();

	let mut content = buffer_from_data_2.write().unwrap();
	//`content` implements `DerefMut` whose target is of type `MyStruct` (the content of the buffer)
	//content as (); //-> vulkano::buffer::cpu_accss::WriteLock<'_, MyStruct>
	content.a *= 2;
	content.b = false;

	let mut array_content = buffer_from_iter.write().unwrap();
	array_content[12] = 83;

	//buffer operations
	//create src/dst buffers
	let source_content = 0 .. 64;
	let src = CpuAccessibleBuffer::from_iter(device.clone(), BufferUsage::all(), source_content).expect("failed to create buffer");

	let dest_content = (0 .. 64).map(|_| 0);
	let dst = CpuAccessibleBuffer::from_iter(device.clone(), BufferUsage::all(), dest_content).expect("failed to create buffer");

	//create a command buffer
	let command_buffer = AutoCommandBufferBuilder::new(device.clone(), queue.family()).unwrap()
		.copy_buffer(src.clone(), dst.clone()).unwrap()
		.build().unwrap();

	//submit the command buffer
	let submitted = command_buffer.execute(queue.clone()).unwrap();

	//submitting an operation doesn't wait for completion
	//use method from the 'submitted' object returned by execute
	submitted.then_signal_fence_and_flush().unwrap().wait(None).unwrap();

	let src_content = src.read().unwrap();
	let dst_content = dst.read().unwrap();
	assert_eq!(&*src_content, &*dst_content);
	println!("GPU copy succeeded!");
}
