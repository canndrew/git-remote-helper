use std::io;
use std::fmt;
use std::marker::PhantomData;
use futures::{Future, Stream, Async};
use void::Void;
use ::{Connect, Ref, Push, Fetch, List, ListForPush, Object, PushObject, FetchObject};

pub fn print_caps<R>() {
    <R as CapConnect>::cap_connect();
    <R as CapPush>::cap_push();
    <R as CapFetch>::cap_fetch();
    println!();
}

trait CapConnect {
    fn cap_connect();
}

impl<T> CapConnect for T {
    default fn cap_connect() {}
}

impl<T: Connect> CapConnect for T {
    fn cap_connect() {
        println!("connect");
    }
}

pub trait CapPush {
    type Fut: Future<Item = (), Error = io::Error> + Send + 'static;

    fn cap_push();
    fn do_push(
        &self,
        objects: &[PushObject],
    ) -> Self::Fut;
}

impl<T> CapPush for T {
    default type Fut = VoidFuture<(), io::Error>;
    default fn cap_push() {}
    default fn do_push(
        &self,
        _objects: &[PushObject],
    ) -> Self::Fut {
        panic!("operation push not supported");
    }
}

impl<T: Push> CapPush for T {
    fn cap_push() {
        println!("push");
    }
}

pub trait CapFetch {
    type Fut: Future<Item = (), Error = io::Error> + Send;

    fn cap_fetch();
    fn do_fetch(
        &self,
        objects: &[FetchObject],
    ) -> Self::Fut;
}

impl<T> CapFetch for T {
    default type Fut = VoidFuture<(), io::Error>;

    default fn cap_fetch() {}
    default fn do_fetch(
        &self,
        _objects: &[FetchObject],
    ) -> Self::Fut {
        panic!("operation fetch not supported");
    }
}

impl<T: Fetch> CapFetch for T {
    type Fut = T::Fut;

    fn cap_fetch() {
        println!("fetch");
    }

    fn do_fetch(
        &self,
        objects: &[FetchObject],
    ) -> T::Fut {
        self.fetch(objects)
    }
}

struct Hexed(pub [u8; 20]);

impl fmt::Display for Hexed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for b in &self.0[..] {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

pub trait CapList: Sized {
    type Fut: Future<Item = (), Error = io::Error> + Send + 'static;

    fn do_list(&mut self) -> Self::Fut;
}

impl<T> CapList for T {
    default type Fut = VoidFuture<(), io::Error>;

    default fn do_list(&mut self) -> Self::Fut {
        panic!("operation list not supported!");
    }
}

impl<T: List> CapList for T {
    type Fut = Box<Future<Item = (), Error = io::Error> + Send + 'static>;

    fn do_list(&mut self) -> Self::Fut {
        let iter = self.list();
        do_list_inner(iter)
    }
}

fn do_list_inner<S: Stream<Item = Ref, Error = io::Error> + Send + 'static>(list: S)
    -> Box<Future<Item = (), Error = io::Error> + Send>
{
    Box::new(
        list
        .for_each(|r| {
            let attrs = if r.unchanged {
                " unchanged"
            } else {
                ""
            };
            match r.object {
                Object::Link(s) => {
                    println!("@{} {}{}", s, r.name, attrs);
                },
                Object::Hash(h) => {
                    println!("{} {}{}", Hexed(h), r.name, attrs);
                },
            }
            Ok(())
        })
    )
}

pub trait CapListForPush {
    type Fut: Future<Item = (), Error = io::Error> + Send + 'static;

    fn do_list_for_push(&mut self) -> Self::Fut;
}

impl<T> CapListForPush for T {
    default type Fut = VoidFuture<(), io::Error>;

    default fn do_list_for_push(&mut self) -> Self::Fut {
        panic!("operation list for-push not supported!");
    }
}

impl<T: ListForPush> CapListForPush for T {
    type Fut = Box<Future<Item = (), Error = io::Error> + Send>;

    fn do_list_for_push(&mut self) -> Self::Fut {
        let iter = self.list_for_push();
        do_list_inner(iter)
    }
}

pub struct VoidFuture<T, E> {
    _ph_t: PhantomData<T>,
    _ph_e: PhantomData<E>,
    void: Void,
}

impl<T, E> Future for VoidFuture<T, E> {
    type Item = T;
    type Error = E;

    fn poll(&mut self) -> Result<Async<T>, E> {
        match self.void {}
    }
}

