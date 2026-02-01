use std::path::Path;
#[cfg(debug_assertions)]
use std::sync::{Arc, Mutex};

use minijinja::Environment;

#[derive(Clone)]
pub struct Jinja(
    #[cfg(debug_assertions)] Arc<Mutex<Environment<'static>>>,
    #[cfg(not(debug_assertions))] Environment<'static>,
);

impl Default for Jinja {
    fn default() -> Self {
        let env = Environment::new();

        #[cfg(debug_assertions)]
        return Self(Arc::new(Mutex::new(env)));

        #[cfg(not(debug_assertions))]
        Self(env)
    }
}

impl Jinja {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let mut env = Environment::new();
        env.set_loader(minijinja::path_loader(path));

        #[cfg(debug_assertions)]
        return Self(Arc::new(Mutex::new(env)));

        #[cfg(not(debug_assertions))]
        Self(env)
    }

    pub fn render(
        &self,
        name: &str,
        ctx: impl serde::Serialize,
    ) -> Result<String, minijinja::Error> {
        #[cfg(debug_assertions)]
        {
            let mut env = self.0.lock().unwrap();
            env.clear_templates();
            env.get_template(name)?.render(&ctx)
        }

        #[cfg(not(debug_assertions))]
        self.0.get_template(name)?.render(&ctx)
    }

    #[cfg(debug_assertions)]
    pub fn with(&mut self, f: impl FnOnce(&mut Environment<'static>)) {
        f(&mut self.0.lock().unwrap());
    }

    #[cfg(not(debug_assertions))]
    pub fn with(&mut self, f: impl FnOnce(&mut Environment<'static>)) {
        f(&mut self.0);
    }
}
