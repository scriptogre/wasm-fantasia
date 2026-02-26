use std::sync::Arc;

use rune::runtime::Vm;
use rune::{Context, Diagnostics, Source, Sources};

/// A minimal wrapper around the Rune scripting engine.
///
/// Compiles Rune source code and executes named functions within it.
pub struct ScriptEngine {
    vm: Vm,
}

impl ScriptEngine {
    /// Compile a Rune script from a source string.
    ///
    /// Returns an error if the script contains syntax or compilation errors.
    pub fn new(source: &str) -> Result<Self, rune::support::Error> {
        let context = Context::with_default_modules()?;
        let runtime = Arc::new(context.runtime()?);

        let mut sources = Sources::new();
        let _ = sources.insert(Source::memory(source)?);

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        let unit = result?;
        let vm = Vm::new(runtime, Arc::new(unit));

        Ok(Self { vm })
    }

    /// Call a named function with no arguments and return the result as an `i64`.
    pub fn call_i64(&mut self, name: &str) -> Result<i64, rune::support::Error> {
        let output = self.vm.call([name], ())?;
        let value: i64 = rune::from_value(output)?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_and_call_trivial_script() {
        let mut engine =
            ScriptEngine::new("pub fn hello() { 42 }").expect("script should compile");
        let result = engine.call_i64("hello").expect("call should succeed");
        assert_eq!(result, 42);
    }
}
