use pyo3::prelude::*;
use tokio::sync::oneshot;

use crate::{
    callbacks::{
        CallbackWrapper,
        callback_impl_run,
        callback_impl_loop_step,
        callback_impl_loop_wake,
        callback_impl_loop_err
    },
    runtime::RuntimeRef,
    ws::{HyperWebsocket, UpgradeData}
};
use super::{
    errors::error_proto,
    io::{RSGIHTTPProtocol as HTTPProtocol, RSGIWebsocketProtocol as WebsocketProtocol},
    types::{RSGIScope as Scope, PyResponse, PyResponseBytes}
};


#[pyclass]
pub(crate) struct CallbackRunnerHTTP {
    proto: Py<HTTPProtocol>,
    event_loop: PyObject,
    context: PyObject,
    cb: PyObject
}

impl CallbackRunnerHTTP {
    pub fn new(
        py: Python,
        cb: CallbackWrapper,
        proto: HTTPProtocol,
        scope: Scope
    ) -> Self {
        let pyproto = Py::new(py, proto).unwrap();
        Self {
            proto: pyproto.clone(),
            event_loop: cb.context.event_loop(py).into(),
            context: cb.context.context(py).into(),
            cb: cb.callback.call1(py, (scope, pyproto)).unwrap()
        }
    }

    fn done(&self, py: Python) {
        if let Ok(mut proto) = self.proto.as_ref(py).try_borrow_mut() {
            if let Some(tx) = proto.tx() {
                let _ = tx.send(
                    PyResponse::Bytes(PyResponseBytes::empty(500, Vec::new()))
                );
            }
        }
    }

    fn err(&self, py: Python) {
        log::warn!("Application callable raised an exception");
        self.done(py)
    }

    callback_impl_run!();
    callback_impl_loop_err!();
}

#[pymethods]
impl CallbackRunnerHTTP {
    fn _loop_step(pyself: PyRef<'_, Self>, py: Python) -> PyResult<PyObject> {
        callback_impl_loop_step!(pyself, py)
    }

    fn _loop_wake(pyself: PyRef<'_, Self>, py: Python, fut: PyObject) -> PyResult<PyObject> {
        callback_impl_loop_wake!(pyself, py, fut)
    }
}

#[pyclass]
pub(crate) struct CallbackRunnerWebsocket {
    proto: Py<WebsocketProtocol>,
    event_loop: PyObject,
    context: PyObject,
    cb: PyObject
}

impl CallbackRunnerWebsocket {
    pub fn new(
        py: Python,
        cb: CallbackWrapper,
        proto: WebsocketProtocol,
        scope: Scope
    ) -> Self {
        let pyproto = Py::new(py, proto).unwrap();
        Self {
            proto: pyproto.clone(),
            event_loop: cb.context.event_loop(py).into(),
            context: cb.context.context(py).into(),
            cb: cb.callback.call1(py, (scope, pyproto)).unwrap()
        }
    }

    fn done(&self, py: Python) {
        if let Ok(mut proto) = self.proto.as_ref(py).try_borrow_mut() {
            if let (Some(tx), res) = proto.tx() {
                let _ = tx.send(res);
            }
        }
    }

    fn err(&self, py: Python) {
        log::warn!("Application callable raised an exception");
        self.done(py)
    }

    callback_impl_run!();
    callback_impl_loop_err!();
}

#[pymethods]
impl CallbackRunnerWebsocket {
    fn _loop_step(pyself: PyRef<'_, Self>, py: Python) -> PyResult<PyObject> {
        callback_impl_loop_step!(pyself, py)
    }

    fn _loop_wake(pyself: PyRef<'_, Self>, py: Python, fut: PyObject) -> PyResult<PyObject> {
        callback_impl_loop_wake!(pyself, py, fut)
    }
}

pub(crate) async fn call_rtb_http(
    cb: CallbackWrapper,
    rt: RuntimeRef,
    req: hyper::Request<hyper::Body>,
    scope: Scope
) -> PyResult<PyResponse> {
    let (tx, rx) = oneshot::channel();
    let protocol = HTTPProtocol::new(rt, tx, req);

    Python::with_gil(|py| {
        CallbackRunnerHTTP::new(py, cb, protocol, scope).run(py)
    })?;

    match rx.await {
        Ok(res) => {
            Ok(res)
        },
        _ => {
            log::error!("RSGI protocol failure");
            error_proto!()
        }
    }
}

pub(crate) async fn call_rtt_http(
    cb: CallbackWrapper,
    rt: RuntimeRef,
    req: hyper::Request<hyper::Body>,
    scope: Scope
) -> PyResult<PyResponse> {
    let (tx, rx) = oneshot::channel();
    let protocol = HTTPProtocol::new(rt, tx, req);

    tokio::task::spawn_blocking(move || {
        Python::with_gil(|py| {
            let _ = CallbackRunnerHTTP::new(py, cb, protocol, scope).run(py);
        });
    });

    match rx.await {
        Ok(res) => {
            Ok(res)
        },
        _ => {
            log::error!("RSGI protocol failure");
            error_proto!()
        }
    }
}

pub(crate) async fn call_rtb_ws(
    cb: CallbackWrapper,
    rt: RuntimeRef,
    ws: HyperWebsocket,
    upgrade: UpgradeData,
    scope: Scope
) -> PyResult<(i32, bool)> {
    let (tx, rx) = oneshot::channel();
    let protocol = WebsocketProtocol::new(rt, tx, ws, upgrade);

    Python::with_gil(|py| {
        CallbackRunnerWebsocket::new(py, cb, protocol, scope).run(py)
    })?;

    match rx.await {
        Ok(res) => {
            Ok(res)
        },
        _ => {
            log::error!("RSGI protocol failure");
            error_proto!()
        }
    }
}

pub(crate) async fn call_rtt_ws(
    cb: CallbackWrapper,
    rt: RuntimeRef,
    ws: HyperWebsocket,
    upgrade: UpgradeData,
    scope: Scope
) -> PyResult<(i32, bool)> {
    let (tx, rx) = oneshot::channel();
    let protocol = WebsocketProtocol::new(rt, tx, ws, upgrade);

    tokio::task::spawn_blocking(move || {
        Python::with_gil(|py| {
            let _ = CallbackRunnerWebsocket::new(py, cb, protocol, scope).run(py);
        });
    });

    match rx.await {
        Ok(res) => {
            Ok(res)
        },
        _ => {
            log::error!("RSGI protocol failure");
            error_proto!()
        }
    }
}
