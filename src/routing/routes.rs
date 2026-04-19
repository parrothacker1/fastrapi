use crate::routing::types::{HttpMethod, RouteHandler, HTTP_METHOD_COUNT};
use ahash::AHashMap;
use pyo3::{Py, PyAny};
use std::sync::Arc;

pub struct FrozenRouter {
    static_routes: [AHashMap<String, Arc<RouteHandler>>; HTTP_METHOD_COUNT],
    param_routes: [matchit::Router<Arc<RouteHandler>>; HTTP_METHOD_COUNT],
    websocket_routes: AHashMap<String, Py<PyAny>>,
}

impl FrozenRouter {
    pub fn resolve<'a>(
        &'a self,
        method: HttpMethod,
        path: &'a str,
    ) -> Option<(Arc<RouteHandler>, Vec<(&'a str, &'a str)>)> {
        let idx = method as usize;

        if let Some(handler) = self.static_routes[idx].get(path) {
            return Some((handler.clone(), Vec::new()));
        }

        match self.param_routes[idx].at(path) {
            Ok(matched) => {
                let params: Vec<_> = matched.params.iter().collect();
                Some((matched.value.clone(), params))
            }
            Err(_) => None,
        }
    }

    pub fn resolve_ws(&self, path: &str) -> Option<Py<PyAny>> {
        self.websocket_routes.get(path).cloned()
    }
}

pub struct FrozenRouterBuilder {
    static_routes: [AHashMap<String, Arc<RouteHandler>>; HTTP_METHOD_COUNT],
    param_entries: [Vec<(String, Arc<RouteHandler>)>; HTTP_METHOD_COUNT],
    websocket_routes: AHashMap<String, Py<PyAny>>,
}

impl FrozenRouterBuilder {
    pub fn new() -> Self {
        Self {
            static_routes: std::array::from_fn(|_| AHashMap::new()),
            param_entries: std::array::from_fn(|_| Vec::new()),
            websocket_routes: AHashMap::new(),
        }
    }

    pub fn add_route(
        &mut self,
        method: HttpMethod,
        path: &str,
        handler: Arc<RouteHandler>,
        is_param: bool,
    ) {
        let idx = method as usize;
        if is_param {
            self.param_entries[idx].push((path.to_string(), handler));
        } else {
            self.static_routes[idx].insert(path.to_string(), handler);
        }
    }

    pub fn add_websocket(&mut self, path: &str, handler: Py<PyAny>) {
        self.websocket_routes.insert(path.to_string(), handler);
    }

    pub fn build(self) -> FrozenRouter {
        let param_routes = {
            let mut arr: [matchit::Router<Arc<RouteHandler>>; HTTP_METHOD_COUNT] =
                std::array::from_fn(|_| matchit::Router::new());
            for (idx, entries) in self.param_entries.into_iter().enumerate() {
                for (path, handler) in entries {
                    if let Err(e) = arr[idx].insert(&path, handler) {
                        tracing::warn!("Failed to insert parameterized route '{}': {}", path, e);
                    }
                }
            }
            arr
        };

        FrozenRouter {
            static_routes: self.static_routes,
            param_routes,
            websocket_routes: self.websocket_routes,
        }
    }
}
