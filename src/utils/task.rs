use crate::log_wrapper::logger::update_thread_logger;

use tokio_bus::{Bus, BusReader};

use std::clone::Clone;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

#[allow(dead_code)]
pub struct Task {
    handler: thread::JoinHandle<()>,
    name: &'static str,
}

#[allow(dead_code)]
pub struct TaskWithInputs<A> {
    task: Task,
    channel_input: TaskMessageBox<A>,
}

pub struct Tasks {
    all_tasks: Vec<Task>,
}
impl Tasks {
    pub fn new() -> Self {
        Tasks {
            all_tasks: Vec::new(),
        }
    }

    pub fn task_create<F>(&mut self, name: &'static str, f: F)
    where
        F: FnOnce() -> (),
        F: Send + 'static,
    {
        info!("starting tasks: {}", name);
        let handler = thread::spawn(move || {
            update_thread_logger(|logger| logger.new(o!("task"=> name.to_string())));
            f()
        });
        let task = Task {
            handler: handler,
            name: name,
        };
        self.all_tasks.push(task);
    }

    pub fn task_create_with_inputs<F, A>(&mut self, name: &'static str, f: F) -> TaskMessageBox<A>
    where
        F: FnOnce(Receiver<A>) -> (),
        F: Send + 'static,
        A: Send + 'static,
    {
        let (tx, rx) = channel();

        self.task_create(name, move || f(rx));

        TaskMessageBox(tx)
    }

    pub fn join(self) {
        for thread in self.all_tasks {
            // TODO
            thread.handler.join().unwrap();
        }
    }
}

pub struct TaskMessageBox<A>(Sender<A>);

impl<A> Clone for TaskMessageBox<A> {
    fn clone(&self) -> Self {
        TaskMessageBox(self.0.clone())
    }
}

impl<A> TaskMessageBox<A> {
    pub fn send_to(&self, a: A) {
        self.0.send(a).unwrap()
    }
}

impl<A> TaskWithInputs<A> {
    pub fn get_message_box(&self) -> TaskMessageBox<A> {
        TaskMessageBox(self.channel_input.0.clone())
    }
}

pub struct TaskBroadcastBox<T: Clone + Sync>(Bus<T>);

impl<T: Clone + Sync> TaskBroadcastBox<T> {
    pub fn new(len: usize) -> Self {
        TaskBroadcastBox(Bus::new(len))
    }

    pub fn add_rx(&mut self) -> BusReader<T> {
        self.0.add_rx()
    }

    pub fn send_broadcast(&mut self, val: T) {
        match self.0.try_broadcast(val) {
            Ok(()) => {}
            Err(_) => panic!("broadcast failed, some network tasks may be blocked"),
        }
    }
}
