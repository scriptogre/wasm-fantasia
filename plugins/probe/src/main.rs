use wasm_component_layer::*;

fn main() {
    let bytes = std::fs::read(std::env::args().nth(1).expect("component path")).unwrap();
    let engine = Engine::new(wasmi_runtime_layer::Engine::default());
    let mut store = Store::new(&engine, ());
    let component = Component::new(&engine, &bytes).unwrap();
    let instance = Linker::default()
        .instantiate(&mut store, &component)
        .unwrap();
    let interface = instance
        .exports()
        .instance(&"fantasia:mechanics/fury@0.1.0".try_into().unwrap())
        .unwrap();
    let bonus = interface
        .func("bonus-percent")
        .unwrap()
        .typed::<i64, u16>()
        .unwrap();
    assert_eq!(bonus.call(&mut store, 12).unwrap(), 144);
    println!("Component runs through Wasmi");
}
