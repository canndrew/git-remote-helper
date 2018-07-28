#![feature(specialization)]
#![feature(nll)]
#![feature(async_await)]

extern crate futures;
extern crate future_utils;
extern crate tokio;
extern crate void;

use std::io;
use std::env;
use std::path::{Path, PathBuf};
use capabilities::{CapList, CapListForPush, CapPush, CapFetch};
use futures::{future::{self, Loop}, Future, Stream};
use future_utils::FutureExt;

mod capabilities;

pub fn run<R: RemoteHelper>() {
    let mut args = env::args_os();
    args.next();
    let remote = match args.next() {
        Some(remote) => remote,
        None => panic!("expected command line argument"),
    };
    let remote = match remote.into_string() {
        Ok(remote) => remote,
        Err(..) => panic!("non-utf8 remote name"),
    };
    let url = match args.next() {
        Some(url) => url,
        None => panic!("expected command line argument"),
    };
    let url = match url.into_string() {
        Ok(url) => url,
        Err(..) => panic!("non-utf8 url"),
    };
    let path: PathBuf = match env::var_os("GIT_DIR") {
        Some(path) => path.into(),
        None => panic!("environment var $GIT_DIR not set"),
    };

    tokio::run(
        R::new(&remote, &url, &path)
        .map_err(|e| panic!("error starting: {}", e))
        .and_then(move |mut remote_helper| {
            let stdin = std::io::stdin();
            let mut line = String::new();
            match stdin.read_line(&mut line) {
                Ok(..) => (),
                Err(..) => panic!("unexpected end of input"),
            };
            if line.trim() != "capabilities" {
                panic!("unexpected input: {}", line);
            }

            capabilities::print_caps::<R>();

            let mut object_path = path.clone();
            object_path.push("objects");

            future::loop_fn((), move |()| {
                let mut line = String::new();
                match stdin.read_line(&mut line) {
                    Ok(..) => (),
                    Err(..) => panic!("unexpected end of input"),
                };

                if line.trim() == "list" {
                    return {
                        remote_helper
                        .do_list()
                        .map_err(|e| panic!("{}", e))
                        .map(|()| Loop::Continue(()))
                        .into_send_boxed()
                    };
                }

                if line.trim() == "list for-push" {
                    return {
                        remote_helper
                        .do_list_for_push()
                        .map_err(|e| panic!("{}", e))
                        .map(|()| Loop::Continue(()))
                        .into_send_boxed()
                    };
                }

                if line.split_whitespace().next() == Some("fetch") {
                    let mut objects = Vec::new();
                    loop {
                        let mut split = line.split_whitespace();
                        let _ = split.next();

                        let sha1_str = match split.next() {
                            Some(sha1_str) => sha1_str,
                            None => panic!("expected sha1 hash"),
                        };
                        let name = match split.next() {
                            Some(name) => name,
                            None => panic!("expected name"),
                        };
                        match split.next() {
                            Some(..) => panic!("unexpected input"),
                            None => (),
                        };

                        let sha1 = parse_object(sha1_str);
                        let mut path = object_path.clone();
                        path.push(&sha1_str[0..2]);
                        path.push(&sha1_str[2..]);

                        objects.push(FetchObject {
                            object: sha1,
                            name: name.to_owned(),
                            path: path,
                        });

                        line = String::new();
                        match stdin.read_line(&mut line) {
                            Ok(..) => (),
                            Err(..) => panic!("unexpected end of input"),
                        };
                        match line.split_whitespace().next() {
                            Some("fetch") => (),
                            Some(..) => panic!("unexpected command"),
                            None => break,
                        }
                    }
                    return {
                        remote_helper
                        .do_fetch(&objects[..])
                        .map_err(|e| panic!("error fetching: {}", e))
                        .map(|()| Loop::Continue(()))
                        .into_send_boxed()
                    };
                }

                if line.split_whitespace().next() == Some("push") {
                    let mut objects = Vec::new();
                    loop {
                        let mut split = line.split_whitespace();
                        let _ = split.next();

                        let words = match split.next() {
                            Some(words) => words,
                            None => panic!("expected push args"),
                        };
                        let forced = words.starts_with('+');
                        let words = {
                            let mut chars = words.chars();
                            chars.next();
                            chars.as_str()
                        };
                        let mut chunks = words.split(':');
                        let src = match chunks.next() {
                            Some(src) => src,
                            None => panic!("expected src"),
                        };
                        let dst = match chunks.next() {
                            Some(dst) => dst,
                            None => panic!("expected dst"),
                        };
                        match chunks.next() {
                            Some(x) => panic!("unexpected arg: {}", x),
                            None => (),
                        }

                        objects.push(PushObject {
                            forced,
                            src: src.to_owned(),
                            dst: dst.to_owned(),
                        });

                        line = String::new();
                        match stdin.read_line(&mut line) {
                            Ok(..) => (),
                            Err(..) => panic!("unexpected end of input"),
                        };
                        match line.split_whitespace().next() {
                            Some("fetch") => (),
                            Some(..) => panic!("unexpected command"),
                            None => break,
                        }
                    }
                    return {
                        remote_helper
                        .do_push(&objects[..])
                        .map_err(|e| panic!("error pushing: {}", e))
                        .map(|()| Loop::Continue(()))
                        .into_send_boxed()
                    };
                }

                future::ok(Loop::Break(())).into_send_boxed()
            })
        })
    )
}

pub trait RemoteHelper: Sized + Send + 'static {
    type Fut: Future<Item = Self, Error = io::Error> + Send + 'static;
    fn new(remote: &str, url: &str, directory: &Path) -> Self::Fut;
}

pub trait Connect {}

pub enum Object {
    Link(String),
    Hash([u8; 20]),
}

pub struct Ref {
    pub object: Object,
    pub name: String,
    pub unchanged: bool,
}

pub trait List {
    type Items: Stream<Item = Ref, Error = io::Error> + Send + 'static;

    fn list(&mut self) -> Self::Items;
}

pub trait ListForPush: List {
    fn list_for_push(&mut self) -> Self::Items;
}

pub struct PushObject {
    pub forced: bool,
    pub src: String,
    pub dst: String,
}

pub trait Push: ListForPush {
    type Fut: Future<Item = (), Error = io::Error> + Send + 'static;
    fn push(&self, objects: &[PushObject]) -> Self::Fut;
}

pub struct FetchObject {
    pub object: Object,
    pub name: String,
    pub path: PathBuf,
}

pub trait Fetch: List {
    type Fut: Future<Item = (), Error = io::Error> + Send;
    fn fetch(&self, objects: &[FetchObject]) -> Self::Fut;
}

fn parse_object(s: &str) -> Object {
    if s.starts_with("@") {
        return Object::Link(s[1..].to_owned());
    }

    let mut ret = [0u8; 20];
    for i in 0..20 {
        ret[i] = match u8::from_str_radix(&s[(i * 2)..(1 + i * 2)], 16) {
            Ok(x) => x,
            Err(..) => panic!("invalid hash"),
        };
    }
    Object::Hash(ret)
}

