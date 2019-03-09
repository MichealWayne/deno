// Copyright 2018-2019 the Deno authors. All rights reserved. MIT license.
#![allow(unused_variables)]
#![allow(dead_code)]

use crate::errors::DenoResult;
use crate::isolate_init::IsolateInit;
use crate::isolate_state::IsolateState;
use crate::modules::Modules;
use crate::msg_ring;
use crate::ops;
use crate::permissions::DenoPermissions;
use deno_core::deno_buf;
use deno_core::deno_mod;
use deno_core::Behavior;
use deno_core::Op;
use std::cell::Cell;
use std::cell::RefCell;
use std::sync::Arc;
use std::time::Instant;

// Buf represents a byte array returned from a "Op". The message might be empty
// (which will be translated into a null object on the javascript side) or it is
// a heap allocated opaque sequence of bytes.  Usually a flatbuffer message.
pub type Buf = Box<[u8]>;

pub type Isolate = deno_core::Isolate<Buf, Cli>;

pub type CliOp = Op<Buf>;

/// Implements deno_core::Behavior for the main Deno command-line.
pub struct Cli {
  shared: Option<deno_buf>, // Pin?
  tx: msg_ring::Sender,
  rx: msg_ring::Receiver,
  init: IsolateInit,
  timeout_due: Cell<Option<Instant>>,
  pub modules: RefCell<Modules>,
  pub state: Arc<IsolateState>,
  pub permissions: Arc<DenoPermissions>,
}

impl Cli {
  pub fn new(
    init: IsolateInit,
    state: Arc<IsolateState>,
    permissions: DenoPermissions,
  ) -> Self {
    let buffer = msg_ring::Buffer::new(1024 * 1024);
    let shared = buffer.into_deno_buf();
    let (tx, rx) = msg_ring::MsgRing::new(buffer).split();
    Self {
      init,
      shared: Some(shared),
      tx,
      rx,
      timeout_due: Cell::new(None),
      modules: RefCell::new(Modules::new()),
      state,
      permissions: Arc::new(permissions),
    }
  }

  #[inline]
  pub fn get_timeout_due(&self) -> Option<Instant> {
    self.timeout_due.clone().into_inner()
  }

  #[inline]
  pub fn set_timeout_due(&self, inst: Option<Instant>) {
    self.timeout_due.set(inst);
  }

  #[inline]
  pub fn check_read(&self, filename: &str) -> DenoResult<()> {
    self.permissions.check_read(filename)
  }

  #[inline]
  pub fn check_write(&self, filename: &str) -> DenoResult<()> {
    self.permissions.check_write(filename)
  }

  #[inline]
  pub fn check_env(&self) -> DenoResult<()> {
    self.permissions.check_env()
  }

  #[inline]
  pub fn check_net(&self, filename: &str) -> DenoResult<()> {
    self.permissions.check_net(filename)
  }

  #[inline]
  pub fn check_run(&self) -> DenoResult<()> {
    self.permissions.check_run()
  }
}

impl Behavior<Buf> for Cli {
  fn startup_snapshot(&mut self) -> Option<deno_buf> {
    self.init.snapshot.take()
  }

  fn startup_shared(&mut self) -> Option<deno_buf> {
    self.shared.take()
  }

  fn resolve(&mut self, specifier: &str, referrer: deno_mod) -> deno_mod {
    unimplemented!()
  }

  fn recv(
    &mut self,
    control: Buf,
    zero_copy_buf: deno_buf,
  ) -> (bool, Box<CliOp>) {
    ops::dispatch(self, control, zero_copy_buf)
  }

  fn records_reset(&mut self) {
    self.tx.reset();
    self.rx.reset();
  }

  fn records_push(&mut self, record: Buf) -> bool {
    let maybe_msg = self.tx.compose(record.len());
    if let Some(mut msg) = maybe_msg {
      msg.copy_from_slice(&record);
      msg.send();
      debug!("compose ok");
      true
    } else {
      debug!("compose fail");
      false
    }
  }

  fn records_shift(&mut self) -> Option<Buf> {
    self.rx.receive().map(|msg| {
      let mut v = Vec::new();
      v.resize(msg.len(), 0);
      let mut bs = v.into_boxed_slice();
      bs.copy_from_slice(&msg[..]);
      bs
    })
  }
}
