(component
  (core module $m
    (func $echo (export "echo") (param i32) (result i32)
      local.get 0))
  (core instance $i (instantiate $m))
  (func $echo (param "value" s32) (result s32)
    (canon lift (core func $i "echo")))
  (instance $echo-interface (export "echo" (func $echo)))
  (export "patina:mct-test/echo@0.1.0" (instance $echo-interface)))
