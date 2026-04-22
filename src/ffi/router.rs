use pyo3::prelude::{pyclass, pymethods, Bound, Py, PyAny, PyAnyMethods, PyResult, Python};
use pyo3::types::{PyCFunction, PyDict, PyTuple};
use std::sync::{Arc, Mutex};

use crate::routing::types::{HttpMethod, RouteEntry, SubRouterMount, WebSocketEntry};

#[pyclass(name = "APIRouter")]
pub struct PyAPIRouter {
    #[pyo3(get)]
    pub prefix: String,
    #[pyo3(get)]
    pub tags: Vec<String>,
    #[pyo3(get)]
    pub dependencies: Option<Py<PyAny>>,
    #[pyo3(get)]
    pub responses: Option<Py<PyAny>>,
    #[pyo3(get)]
    pub deprecated: Option<bool>,
    #[pyo3(get)]
    pub include_in_schema: bool,
    #[pyo3(get)]
    pub default_response_class: Option<Py<PyAny>>,
    #[pyo3(get)]
    pub generate_unique_id_function: Option<Py<PyAny>>,

    // Internal mutable storage
    pub route_entries: Arc<Mutex<Vec<RouteEntry>>>,
    pub websocket_entries: Arc<Mutex<Vec<WebSocketEntry>>>,
    pub sub_routers: Arc<Mutex<Vec<SubRouterMount>>>,
}

impl PyAPIRouter {
    pub fn create_method_decorator(
        &self,
        py: Python<'_>,
        method: HttpMethod,
        path: String,
        tags: Option<Py<PyAny>>,
        summary: Option<String>,
        description: Option<String>,
        deprecated: Option<bool>,
        include_in_schema: bool,
    ) -> PyResult<Py<PyAny>> {
        let mut merged_tags = self.tags.clone();
        if let Some(route_tags) = tags {
            let tag_list = route_tags.bind(py);
            if let Ok(iter) = tag_list.try_iter() {
                for item in iter.flatten() {
                    if let Ok(tag) = item.extract::<String>() {
                        if !merged_tags.contains(&tag) {
                            merged_tags.push(tag);
                        }
                    }
                }
            }
        }
        let deprecated = deprecated.or(self.deprecated);
        let path_for_closure = path.clone();
        let routes = Arc::clone(&self.route_entries);
        let decorator = move |args: &Bound<'_, PyTuple>,
                              _kwargs: Option<&Bound<'_, PyDict>>|
              -> PyResult<Py<PyAny>> {
            let py = args.py();
            let func: Py<PyAny> = args.get_item(0)?.unbind();
            let metadata =
                crate::ffi::pydantic::parse_route_metadata(py, &func.bind(py), &path_for_closure);
            let needs_kwargs = !metadata.path_param_names.is_empty()
                || !metadata.query_param_names.is_empty()
                || !metadata.body_param_names.is_empty()
                || !metadata.param_validators.is_empty()
                || !metadata.dependencies.is_empty()
                || !metadata.parsed_params.is_empty();
            let handler = Arc::new(crate::routing::types::RouteHandler {
                func: func.clone_ref(py),
                is_async: metadata.is_async,
                is_fast_path: metadata.is_fast_path,
                dependency_needs_request: metadata.dependency_needs_request,
                needs_kwargs,
                param_validators: metadata.param_validators,
                response_type: metadata.response_type,
                path_param_names: metadata.path_param_names,
                query_param_names: metadata.query_param_names,
                body_param_names: metadata.body_param_names,
                dependencies: metadata.dependencies,
                parsed_params: metadata.parsed_params,
            });
            let entry = RouteEntry {
                method,
                path: path_for_closure.clone(),
                handler,
                tags: merged_tags.clone(),
                summary: summary.clone(),
                description: description.clone(),
                deprecated,
                include_in_schema,
            };
            routes.lock().unwrap().push(entry);
            Ok(func)
        };
        PyCFunction::new_closure(py, None, None, decorator).map(|f| f.into())
    }

    pub fn create_ws_decorator(&self, py: Python<'_>, path: String) -> PyResult<Py<PyAny>> {
        if !path.starts_with('/') {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "WebSocket path must start with '/'",
            ));
        }
        let websockets = Arc::clone(&self.websocket_entries);
        let closure = move |args: &Bound<'_, PyTuple>,
                            _kwargs: Option<&Bound<'_, PyDict>>|
              -> PyResult<Py<PyAny>> {
            let py = args.py();
            let func: Py<PyAny> = args.get_item(0)?.unbind();
            let entry = WebSocketEntry {
                path: path.clone(),
                handler: func.clone_ref(py),
            };
            websockets.lock().unwrap().push(entry);
            Ok(func)
        };
        PyCFunction::new_closure(py, None, None, closure).map(|f| f.into())
    }
}

#[pymethods]
impl PyAPIRouter {
    #[new]
    #[pyo3(signature = (*, prefix="".to_string(), tags=None, dependencies=None, responses=None, deprecated=None, include_in_schema=true, default_response_class=None, generate_unique_id_function=None))]
    fn new(
        prefix: String,
        tags: Option<Py<PyAny>>,
        dependencies: Option<Py<PyAny>>,
        responses: Option<Py<PyAny>>,
        deprecated: Option<bool>,
        include_in_schema: bool,
        default_response_class: Option<Py<PyAny>>,
        generate_unique_id_function: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let tag_vec = Python::attach(|py| {
            if let Some(ref tags_obj) = tags {
                let tags_bound = tags_obj.bind(py);
                if let Ok(iter) = tags_bound.try_iter() {
                    return iter
                        .filter_map(|item| item.ok()?.extract::<String>().ok())
                        .collect();
                }
            }
            Vec::new()
        });

        Ok(Self {
            prefix,
            tags: tag_vec,
            dependencies,
            responses,
            deprecated,
            include_in_schema,
            default_response_class,
            generate_unique_id_function,
            route_entries: Arc::new(Mutex::new(Vec::new())),
            websocket_entries: Arc::new(Mutex::new(Vec::new())),
            sub_routers: Arc::new(Mutex::new(Vec::new())),
        })
    }

    #[pyo3(signature = (path, *, response_model=None, status_code=None, tags=None, dependencies=None, summary=None, description=None, response_description=None, responses=None, deprecated=None, operation_id=None, response_model_include=None, response_model_exclude=None, response_model_by_alias=true, response_model_exclude_unset=false, response_model_exclude_defaults=false, response_model_exclude_none=false, include_in_schema=true, response_class=None, name=None, callbacks=None, openapi_extra=None, generate_unique_id_function=None))]
    #[allow(unused_variables)]
    fn get(
        &self,
        py: Python<'_>,
        path: String,
        response_model: Option<Py<PyAny>>,
        status_code: Option<u16>,
        tags: Option<Py<PyAny>>,
        dependencies: Option<Py<PyAny>>,
        summary: Option<String>,
        description: Option<String>,
        response_description: Option<String>,
        responses: Option<Py<PyAny>>,
        deprecated: Option<bool>,
        operation_id: Option<String>,
        response_model_include: Option<Py<PyAny>>,
        response_model_exclude: Option<Py<PyAny>>,
        response_model_by_alias: bool,
        response_model_exclude_unset: bool,
        response_model_exclude_defaults: bool,
        response_model_exclude_none: bool,
        include_in_schema: bool,
        response_class: Option<Py<PyAny>>,
        name: Option<String>,
        callbacks: Option<Py<PyAny>>,
        openapi_extra: Option<Py<PyAny>>,
        generate_unique_id_function: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        self.create_method_decorator(
            py,
            HttpMethod::GET,
            path,
            tags,
            summary,
            description,
            deprecated,
            include_in_schema,
        )
    }

    #[pyo3(signature = (path, *, response_model=None, status_code=None, tags=None, dependencies=None, summary=None, description=None, response_description=None, responses=None, deprecated=None, operation_id=None, response_model_include=None, response_model_exclude=None, response_model_by_alias=true, response_model_exclude_unset=false, response_model_exclude_defaults=false, response_model_exclude_none=false, include_in_schema=true, response_class=None, name=None, callbacks=None, openapi_extra=None, generate_unique_id_function=None))]
    #[allow(unused_variables)]
    fn post(
        &self,
        py: Python<'_>,
        path: String,
        response_model: Option<Py<PyAny>>,
        status_code: Option<u16>,
        tags: Option<Py<PyAny>>,
        dependencies: Option<Py<PyAny>>,
        summary: Option<String>,
        description: Option<String>,
        response_description: Option<String>,
        responses: Option<Py<PyAny>>,
        deprecated: Option<bool>,
        operation_id: Option<String>,
        response_model_include: Option<Py<PyAny>>,
        response_model_exclude: Option<Py<PyAny>>,
        response_model_by_alias: bool,
        response_model_exclude_unset: bool,
        response_model_exclude_defaults: bool,
        response_model_exclude_none: bool,
        include_in_schema: bool,
        response_class: Option<Py<PyAny>>,
        name: Option<String>,
        callbacks: Option<Py<PyAny>>,
        openapi_extra: Option<Py<PyAny>>,
        generate_unique_id_function: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        self.create_method_decorator(
            py,
            HttpMethod::POST,
            path,
            tags,
            summary,
            description,
            deprecated,
            include_in_schema,
        )
    }

    #[pyo3(signature = (path, *, response_model=None, status_code=None, tags=None, dependencies=None, summary=None, description=None, response_description=None, responses=None, deprecated=None, operation_id=None, response_model_include=None, response_model_exclude=None, response_model_by_alias=true, response_model_exclude_unset=false, response_model_exclude_defaults=false, response_model_exclude_none=false, include_in_schema=true, response_class=None, name=None, callbacks=None, openapi_extra=None, generate_unique_id_function=None))]
    #[allow(unused_variables)]
    fn put(
        &self,
        py: Python<'_>,
        path: String,
        response_model: Option<Py<PyAny>>,
        status_code: Option<u16>,
        tags: Option<Py<PyAny>>,
        dependencies: Option<Py<PyAny>>,
        summary: Option<String>,
        description: Option<String>,
        response_description: Option<String>,
        responses: Option<Py<PyAny>>,
        deprecated: Option<bool>,
        operation_id: Option<String>,
        response_model_include: Option<Py<PyAny>>,
        response_model_exclude: Option<Py<PyAny>>,
        response_model_by_alias: bool,
        response_model_exclude_unset: bool,
        response_model_exclude_defaults: bool,
        response_model_exclude_none: bool,
        include_in_schema: bool,
        response_class: Option<Py<PyAny>>,
        name: Option<String>,
        callbacks: Option<Py<PyAny>>,
        openapi_extra: Option<Py<PyAny>>,
        generate_unique_id_function: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        self.create_method_decorator(
            py,
            HttpMethod::PUT,
            path,
            tags,
            summary,
            description,
            deprecated,
            include_in_schema,
        )
    }

    #[pyo3(signature = (path, *, response_model=None, status_code=None, tags=None, dependencies=None, summary=None, description=None, response_description=None, responses=None, deprecated=None, operation_id=None, response_model_include=None, response_model_exclude=None, response_model_by_alias=true, response_model_exclude_unset=false, response_model_exclude_defaults=false, response_model_exclude_none=false, include_in_schema=true, response_class=None, name=None, callbacks=None, openapi_extra=None, generate_unique_id_function=None))]
    #[allow(unused_variables)]
    fn delete(
        &self,
        py: Python<'_>,
        path: String,
        response_model: Option<Py<PyAny>>,
        status_code: Option<u16>,
        tags: Option<Py<PyAny>>,
        dependencies: Option<Py<PyAny>>,
        summary: Option<String>,
        description: Option<String>,
        response_description: Option<String>,
        responses: Option<Py<PyAny>>,
        deprecated: Option<bool>,
        operation_id: Option<String>,
        response_model_include: Option<Py<PyAny>>,
        response_model_exclude: Option<Py<PyAny>>,
        response_model_by_alias: bool,
        response_model_exclude_unset: bool,
        response_model_exclude_defaults: bool,
        response_model_exclude_none: bool,
        include_in_schema: bool,
        response_class: Option<Py<PyAny>>,
        name: Option<String>,
        callbacks: Option<Py<PyAny>>,
        openapi_extra: Option<Py<PyAny>>,
        generate_unique_id_function: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        self.create_method_decorator(
            py,
            HttpMethod::DELETE,
            path,
            tags,
            summary,
            description,
            deprecated,
            include_in_schema,
        )
    }

    #[pyo3(signature = (path, *, response_model=None, status_code=None, tags=None, dependencies=None, summary=None, description=None, response_description=None, responses=None, deprecated=None, operation_id=None, response_model_include=None, response_model_exclude=None, response_model_by_alias=true, response_model_exclude_unset=false, response_model_exclude_defaults=false, response_model_exclude_none=false, include_in_schema=true, response_class=None, name=None, callbacks=None, openapi_extra=None, generate_unique_id_function=None))]
    #[allow(unused_variables)]
    fn patch(
        &self,
        py: Python<'_>,
        path: String,
        response_model: Option<Py<PyAny>>,
        status_code: Option<u16>,
        tags: Option<Py<PyAny>>,
        dependencies: Option<Py<PyAny>>,
        summary: Option<String>,
        description: Option<String>,
        response_description: Option<String>,
        responses: Option<Py<PyAny>>,
        deprecated: Option<bool>,
        operation_id: Option<String>,
        response_model_include: Option<Py<PyAny>>,
        response_model_exclude: Option<Py<PyAny>>,
        response_model_by_alias: bool,
        response_model_exclude_unset: bool,
        response_model_exclude_defaults: bool,
        response_model_exclude_none: bool,
        include_in_schema: bool,
        response_class: Option<Py<PyAny>>,
        name: Option<String>,
        callbacks: Option<Py<PyAny>>,
        openapi_extra: Option<Py<PyAny>>,
        generate_unique_id_function: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        self.create_method_decorator(
            py,
            HttpMethod::PATCH,
            path,
            tags,
            summary,
            description,
            deprecated,
            include_in_schema,
        )
    }

    #[pyo3(signature = (path, *, response_model=None, status_code=None, tags=None, dependencies=None, summary=None, description=None, response_description=None, responses=None, deprecated=None, operation_id=None, response_model_include=None, response_model_exclude=None, response_model_by_alias=true, response_model_exclude_unset=false, response_model_exclude_defaults=false, response_model_exclude_none=false, include_in_schema=true, response_class=None, name=None, callbacks=None, openapi_extra=None, generate_unique_id_function=None))]
    #[allow(unused_variables)]
    fn options(
        &self,
        py: Python<'_>,
        path: String,
        response_model: Option<Py<PyAny>>,
        status_code: Option<u16>,
        tags: Option<Py<PyAny>>,
        dependencies: Option<Py<PyAny>>,
        summary: Option<String>,
        description: Option<String>,
        response_description: Option<String>,
        responses: Option<Py<PyAny>>,
        deprecated: Option<bool>,
        operation_id: Option<String>,
        response_model_include: Option<Py<PyAny>>,
        response_model_exclude: Option<Py<PyAny>>,
        response_model_by_alias: bool,
        response_model_exclude_unset: bool,
        response_model_exclude_defaults: bool,
        response_model_exclude_none: bool,
        include_in_schema: bool,
        response_class: Option<Py<PyAny>>,
        name: Option<String>,
        callbacks: Option<Py<PyAny>>,
        openapi_extra: Option<Py<PyAny>>,
        generate_unique_id_function: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        self.create_method_decorator(
            py,
            HttpMethod::OPTIONS,
            path,
            tags,
            summary,
            description,
            deprecated,
            include_in_schema,
        )
    }

    #[pyo3(signature = (path, *, response_model=None, status_code=None, tags=None, dependencies=None, summary=None, description=None, response_description=None, responses=None, deprecated=None, operation_id=None, response_model_include=None, response_model_exclude=None, response_model_by_alias=true, response_model_exclude_unset=false, response_model_exclude_defaults=false, response_model_exclude_none=false, include_in_schema=true, response_class=None, name=None, callbacks=None, openapi_extra=None, generate_unique_id_function=None))]
    #[allow(unused_variables)]
    fn head(
        &self,
        py: Python<'_>,
        path: String,
        response_model: Option<Py<PyAny>>,
        status_code: Option<u16>,
        tags: Option<Py<PyAny>>,
        dependencies: Option<Py<PyAny>>,
        summary: Option<String>,
        description: Option<String>,
        response_description: Option<String>,
        responses: Option<Py<PyAny>>,
        deprecated: Option<bool>,
        operation_id: Option<String>,
        response_model_include: Option<Py<PyAny>>,
        response_model_exclude: Option<Py<PyAny>>,
        response_model_by_alias: bool,
        response_model_exclude_unset: bool,
        response_model_exclude_defaults: bool,
        response_model_exclude_none: bool,
        include_in_schema: bool,
        response_class: Option<Py<PyAny>>,
        name: Option<String>,
        callbacks: Option<Py<PyAny>>,
        openapi_extra: Option<Py<PyAny>>,
        generate_unique_id_function: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        self.create_method_decorator(
            py,
            HttpMethod::HEAD,
            path,
            tags,
            summary,
            description,
            deprecated,
            include_in_schema,
        )
    }

    #[pyo3(signature = (path))]
    fn websocket(&self, py: Python<'_>, path: String) -> PyResult<Py<PyAny>> {
        self.create_ws_decorator(py, path)
    }

    #[pyo3(signature = (router, *, prefix="".to_string(), tags=None))]
    fn include_router(
        &self,
        py: Python<'_>,
        router: Py<PyAny>,
        prefix: String,
        tags: Option<Py<PyAny>>,
    ) -> PyResult<()> {
        let tag_vec: Vec<String> = if let Some(ref tags_obj) = tags {
            let tags_bound = tags_obj.bind(py);
            if let Ok(iter) = tags_bound.try_iter() {
                iter.filter_map(|item| item.ok()?.extract::<String>().ok())
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        self.sub_routers.lock().unwrap().push(SubRouterMount {
            router: router.clone_ref(py),
            prefix,
            tags: tag_vec,
        });

        Ok(())
    }
}
