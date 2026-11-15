(component
  (import "wasi:sockets/ip-name-lookup@0.2.3" (instance $ambient))
  (core module $m
    (func $double (export "double") (param i32) (result i32)
      local.get 0
      i32.const 2
      i32.mul))
  (core instance $i (instantiate $m))
  (func $double (param "value" s32) (result s32) (canon lift (core func $i "double")))
  (instance $control (export "double" (func $double)))
  (export "patina:demo/control@0.1.0" (instance $control)))
