use crate::routing::types::{HttpMethod, RouteHandler, HTTP_METHOD_COUNT};
use ahash::AHashMap;
use pyo3::{Py, PyAny};
use std::{borrow::Cow, sync::Arc};

pub enum RouteMatch<'a> {
    Static(Arc<RouteHandler>),
    Params(Arc<RouteHandler>, matchit::Params<'a, 'a>),
}

#[derive(Clone)]
pub struct FrozenRouter {
    static_routes: [AHashMap<String, Arc<RouteHandler>>; HTTP_METHOD_COUNT],
    param_routes: [matchit::Router<Arc<RouteHandler>>; HTTP_METHOD_COUNT],
    websocket_routes: AHashMap<String, Py<PyAny>>,
}

impl FrozenRouter {
    #[inline(always)]
    pub fn resolve<'a>(&'a self, method: HttpMethod, path: &'a str) -> Option<RouteMatch<'a>> {
        let idx = method as usize;
        if let Some(handler) = self.static_routes[idx].get(path) {
            return Some(RouteMatch::Static(handler.clone()));
        }
        let matched = self.param_routes[idx].at(path).ok()?;
        Some(RouteMatch::Params(matched.value.clone(), matched.params))
    }

    pub fn resolve_ws(&self, path: &str) -> Option<Py<PyAny>> {
        let normalized = normalize_lookup(path);
        self.websocket_routes.get(normalized.as_ref()).cloned()
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

    pub fn add_route(&mut self, method: HttpMethod, path: String, handler: Arc<RouteHandler>) {
        let idx = method as usize;
        let (normalized, has_params) = normalize_register(&path);

        if has_params {
            self.param_entries[idx].push((normalized.into_owned(), handler));
        } else {
            self.static_routes[idx].insert(normalized.into_owned(), handler);
        }
    }

    pub fn add_websocket(&mut self, path: String, handler: Py<PyAny>) {
        let (normalized, _) = normalize_register(&path);
        self.websocket_routes
            .insert(normalized.into_owned(), handler);
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

impl Default for FrozenRouterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_lookup(input: &str) -> Cow<'_, str> {
    let trimmed = input.trim();
    let needs_leading = !trimmed.starts_with('/');
    let needs_trailing_strip = trimmed.len() > 1 && trimmed.ends_with('/');

    if !needs_leading && !needs_trailing_strip && trimmed.len() == input.len() {
        return Cow::Borrowed(input);
    }
    if !needs_leading && !needs_trailing_strip {
        return Cow::Borrowed(trimmed);
    }

    let mut path = String::with_capacity(trimmed.len() + 1);
    if needs_leading {
        path.push('/');
    }
    path.push_str(trimmed);
    if path.len() > 1 && path.ends_with('/') {
        path.pop();
    }
    Cow::Owned(path)
}

fn normalize_register(input: &str) -> (Cow<'_, str>, bool) {
    let base = normalize_lookup(input);
    if !base.contains('{') {
        return (base, false);
    }

    let mut normalized = String::with_capacity(base.len());
    let mut chars = base.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            normalized.push(':');
            let mut found_close = false;
            for next in chars.by_ref() {
                if next == '}' {
                    found_close = true;
                    break;
                }
                normalized.push(next);
            }
            if !found_close {
                panic!("Invalid route: missing closing '}}' in {}", input);
            }
        } else {
            normalized.push(c);
        }
    }

    (Cow::Owned(normalized), true)
}
