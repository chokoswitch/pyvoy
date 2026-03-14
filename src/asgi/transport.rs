use std::{str::FromStr, sync::Arc};

use envoy_proxy_dynamic_modules_rust_sdk::EnvoyHttpFilterScheduler;
use http::{HeaderName, HeaderValue};
use pyo3::{
    Bound, Py, PyAny, PyResult, Python,
    exceptions::PyValueError,
    pybacked::PyBackedBytes,
    pyclass, pymethods,
    sync::MutexExt,
    types::{PyAnyMethods as _, PyString, PyStringMethods},
};
use pyqwest::internal_for_pyvoy::{Content, Request};

use crate::{
    asgi::python::{self, EVENT_ID_OUTGOING_REQUEST, LoopFuture},
    eventbridge::EventBridge,
    types::Constants,
};

struct CanceledFuture {
    future: Option<LoopFuture>,
    executor: python::Executor,
}

impl Drop for CanceledFuture {
    fn drop(&mut self) {
        if let Some(future) = self.future.take() {
            self.executor.handle_canceled_future(future);
        }
    }
}

enum RequestBody {
    Empty,
    Buffered(PyBackedBytes),
    Iter(Py<PyAny>),
}

struct StartStreamEvent {
    pub headers: Vec<(HeaderName, HeaderValue)>,
    pub body: RequestBody,
}

pub(super) enum TransportEvent {
    Start(StartStreamEvent),
}

#[pyclass(module = "_pyvoy.httpclient")]
struct HTTPTransport {
    loop_: Py<PyAny>,
    bridge: EventBridge<TransportEvent>,
    scheduler: Arc<Box<dyn EnvoyHttpFilterScheduler>>,
    constants: Arc<Constants>,
}

#[pymethods]
impl HTTPTransport {
    fn execute<'py>(&self, py: Python<'py>, request: &Request) -> PyResult<Bound<'py, PyAny>> {
        let future = self
            .loop_
            .bind(py)
            .call_method0(&self.constants.create_future)?;

        let req_headers = request
            .head
            .headers
            .get()
            .store
            .lock_py_attached(py)
            .unwrap();

        let mut headers = Vec::with_capacity(req_headers.len() + 3);
        if let Ok(method_hdr) = HeaderValue::from_str(request.head.method.as_str()) {
            headers.push((HeaderName::from_static(":method"), method_hdr));
        }

        if let Ok(path_hdr) = HeaderValue::from_str(
            &request.head.url[url::Position::BeforePath..url::Position::AfterQuery],
        ) {
            headers.push((HeaderName::from_static(":path"), path_hdr));
        }
        if let Ok(host_hdr) = HeaderValue::from_str(request.head.url.host_str().unwrap_or_default())
        {
            headers.push((HeaderName::from_static("host"), host_hdr));
        }

        for (name, value) in req_headers.iter() {
            headers.push((name.clone(), value.as_http(py)?));
        }

        let body = match &request.content {
            None => RequestBody::Empty,
            Some(Content::Bytes(bytes)) => RequestBody::Buffered(bytes.clone_ref(py)),
            Some(Content::AsyncIter(iter)) => RequestBody::Iter(iter.clone_ref(py)),
        };

        if self
            .bridge
            .send(TransportEvent::Start(StartStreamEvent { headers, body }))
            .is_ok()
        {
            self.scheduler.commit(EVENT_ID_OUTGOING_REQUEST);
        }

        Ok(future)
    }
}
