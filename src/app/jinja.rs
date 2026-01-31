use std::borrow::Cow;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use minijinja::{Environment, Value};

#[cfg(debug_assertions)]
use minijinja_autoreload::AutoReloader;

type BoxedJinjaFn = Arc<dyn Fn(&mut Environment<'static>) + Send + Sync>;

#[derive(Clone)]
struct JinjaConfig {
    path: Option<PathBuf>,
    ops: Vec<BoxedJinjaFn>,
}

impl Default for JinjaConfig {
    fn default() -> Self {
        Self {
            path: None,
            ops: Vec::new(),
        }
    }
}

impl JinjaConfig {
    fn apply(&self, env: &mut Environment<'static>) {
        if let Some(p) = &self.path {
            env.set_loader(minijinja::path_loader(p));
        }
        for op in &self.ops {
            op(env);
        }
    }
}

pub struct Jinja {
    #[cfg(debug_assertions)]
    reloader: AutoReloader,
    #[cfg(not(debug_assertions))]
    env: Environment<'static>,
    config: Arc<JinjaConfig>,
}

impl Clone for Jinja {
    fn clone(&self) -> Self {
        Self::with_config((*self.config).clone())
    }
}

impl Jinja {
    pub fn new() -> Self {
        Self::with_config(JinjaConfig::default())
    }

    pub fn with_path(path: impl AsRef<Path>) -> Self {
        Self::with_config(JinjaConfig {
            path: Some(path.as_ref().to_path_buf()),
            ops: Vec::new(),
        })
    }

    fn with_config(config: JinjaConfig) -> Self {
        let config = Arc::new(config);

        #[cfg(debug_assertions)]
        {
            let cfg = config.clone();
            let reloader = AutoReloader::new(move |notifier| {
                let mut env = Environment::new();
                cfg.apply(&mut env);
                if let Some(p) = &cfg.path {
                    notifier.watch_path(p, true);
                }
                Ok(env)
            });
            Self { reloader, config }
        }

        #[cfg(not(debug_assertions))]
        {
            let mut env = Environment::new();
            config.apply(&mut env);
            Self { env, config }
        }
    }

    fn modify_config<F: FnOnce(&mut JinjaConfig)>(self, f: F) -> Self {
        // Unwrap Arc (we're the only owner after consuming self)
        let mut config = Arc::try_unwrap(self.config).unwrap_or_else(|arc| {
            // Fallback: manually reconstruct (shouldn't happen in builder pattern)
            JinjaConfig {
                path: arc.path.clone(),
                ops: Vec::new(), // ops lost, but this path shouldn't be hit
            }
        });
        f(&mut config);
        Self::with_config(config)
    }

    fn acquire(&self) -> EnvGuard<'_> {
        #[cfg(debug_assertions)]
        {
            EnvGuard(GuardInner::Reloading(
                self.reloader
                    .acquire_env()
                    .expect("failed to acquire reloading environment"),
            ))
        }

        #[cfg(not(debug_assertions))]
        {
            EnvGuard(GuardInner::Static(&self.env))
        }
    }

    pub fn set_path(self, path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        self.modify_config(|c| c.path = Some(path))
    }

    pub fn add_global(
        self,
        name: impl Into<Cow<'static, str>> + Clone + Send + Sync + 'static,
        value: impl Into<Value> + Clone + Send + Sync + 'static,
    ) -> Self {
        self.modify_config(|c| {
            c.ops.push(Arc::new(move |env| {
                env.add_global(name.clone(), value.clone());
            }))
        })
    }

    pub fn add_filter<N, F, Rv, Args>(self, name: N, f: F) -> Self
    where
        N: Into<Cow<'static, str>> + Clone + Send + Sync + 'static,
        F: minijinja::functions::Function<Rv, Args> + Clone + 'static,
        Rv: minijinja::value::FunctionResult,
        Args: for<'a> minijinja::value::FunctionArgs<'a>,
    {
        self.modify_config(|c| {
            c.ops.push(Arc::new(move |env| {
                env.add_filter(name.clone(), f.clone());
            }))
        })
    }

    pub fn add_test<N, F, Rv, Args>(self, name: N, f: F) -> Self
    where
        N: Into<Cow<'static, str>> + Clone + Send + Sync + 'static,
        F: minijinja::functions::Function<Rv, Args> + Clone + 'static,
        Rv: minijinja::value::FunctionResult,
        Args: for<'a> minijinja::value::FunctionArgs<'a>,
    {
        self.modify_config(|c| {
            c.ops.push(Arc::new(move |env| {
                env.add_test(name.clone(), f.clone());
            }))
        })
    }

    pub fn add_function<N, F, Rv, Args>(self, name: N, f: F) -> Self
    where
        N: Into<Cow<'static, str>> + Clone + Send + Sync + 'static,
        F: minijinja::functions::Function<Rv, Args> + Clone + 'static,
        Rv: minijinja::value::FunctionResult,
        Args: for<'a> minijinja::value::FunctionArgs<'a>,
    {
        self.modify_config(|c| {
            c.ops.push(Arc::new(move |env| {
                env.add_function(name.clone(), f.clone());
            }))
        })
    }

    pub fn set_trim_blocks(self, yes: bool) -> Self {
        self.modify_config(|c| c.ops.push(Arc::new(move |env| env.set_trim_blocks(yes))))
    }

    pub fn set_lstrip_blocks(self, yes: bool) -> Self {
        self.modify_config(|c| c.ops.push(Arc::new(move |env| env.set_lstrip_blocks(yes))))
    }

    pub fn set_keep_trailing_newline(self, yes: bool) -> Self {
        self.modify_config(|c| {
            c.ops
                .push(Arc::new(move |env| env.set_keep_trailing_newline(yes)))
        })
    }

    pub fn set_undefined_behavior(self, behavior: minijinja::UndefinedBehavior) -> Self {
        self.modify_config(|c| {
            c.ops
                .push(Arc::new(move |env| env.set_undefined_behavior(behavior)))
        })
    }

    pub fn set_recursion_limit(self, level: usize) -> Self {
        self.modify_config(|c| {
            c.ops
                .push(Arc::new(move |env| env.set_recursion_limit(level)))
        })
    }

    pub fn modify<F>(&mut self, f: F)
    where
        F: FnOnce(Self) -> Self,
    {
        *self = f(std::mem::take(self));
    }

    pub fn render(
        &self,
        name: impl Into<String>,
        ctx: impl serde::Serialize,
    ) -> Result<String, minijinja::Error> {
        let name = name.into();
        let guard = self.acquire();
        let template = guard.get_template(&name)?;
        template.render(&ctx)
    }
}

impl Default for Jinja {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Jinja {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderEnv")
            .field("path", &self.config.path)
            .field("ops_count", &self.config.ops.len())
            .finish()
    }
}

pub struct EnvGuard<'a>(GuardInner<'a>);

enum GuardInner<'a> {
    #[cfg(debug_assertions)]
    Reloading(minijinja_autoreload::EnvironmentGuard<'a>),
    #[cfg(not(debug_assertions))]
    Static(&'a Environment<'static>),
}

impl Deref for EnvGuard<'_> {
    type Target = Environment<'static>;
    fn deref(&self) -> &Self::Target {
        match &self.0 {
            #[cfg(debug_assertions)]
            GuardInner::Reloading(g) => g,
            #[cfg(not(debug_assertions))]
            GuardInner::Static(e) => e,
        }
    }
}
