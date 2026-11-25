//! The verbs of `ap`: each module is one verb's code, except `host`, which the verbs share.

pub mod host;

pub mod review;

pub mod run;

pub mod gate;

pub mod unit;

pub mod deliver;

pub mod store;

pub mod request;

pub mod intent;

pub mod status;

pub mod quota;

pub mod work;

pub mod policy;

pub mod watch;

pub mod check;

pub mod door;

pub mod route;

pub mod clause;

pub mod resource;

pub mod api;

pub mod measure;

pub mod client;
