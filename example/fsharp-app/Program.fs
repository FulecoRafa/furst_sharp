module Program

open System
open System.Runtime.InteropServices
open System.Text
open FurstBindings

// ─── String helpers ───────────────────────────────────────────────────────

/// Pin a .NET string as UTF-8 bytes and pass its (ptr, len) to a Rust function.
let withRustStr (s: string) (f: nativeint -> unativeint -> 'a) : 'a =
    let bytes = Encoding.UTF8.GetBytes(s)
    let handle = GCHandle.Alloc(bytes, GCHandleType.Pinned)
    try f (handle.AddrOfPinnedObject()) (unativeint bytes.Length)
    finally handle.Free()

/// Convert a FurstStr returned from Rust into a .NET string, then free it.
let furstStrToString (s: FurstStr) : string =
    let result = Marshal.PtrToStringUTF8(s.ptr, int s.len)
    furst_free_string s
    result

// ─── Entry point ──────────────────────────────────────────────────────────

[<EntryPoint>]
let main _argv =
    printfn "FurstSharp — P/Invoke demo"
    printfn "═══════════════════════════════════════"

    // ── 1. Primitives: fibonacci ──────────────────────────────────────────
    printfn ""
    printfn "[ primitives — fibonacci(n: i64) -> i64 ]"
    for n in [ 0L; 1L; 5L; 10L; 20L ] do
        printfn "  fibonacci(%2d) = %d" n (fibonacci n)

    // ── 2. Struct: Point + distance ───────────────────────────────────────
    printfn ""
    printfn "[ struct — distance(a: Point, b: Point) -> f64 ]"
    let mutable origin = Point()
    origin.x <- 0.0
    origin.y <- 0.0
    let mutable pt = Point()
    pt.x <- 3.0
    pt.y <- 4.0
    printfn "  distance((0,0), (3,4)) = %.1f" (distance(origin, pt))   // 5.0

    let mutable a = Point()
    a.x <- 1.0
    a.y <- 1.0
    let mutable b = Point()
    b.x <- 4.0
    b.y <- 5.0
    printfn "  distance((1,1), (4,5)) = %.1f" (distance(a, b))         // 5.0

    // ── 3. C-style enum: Direction + turn_right ───────────────────────────
    printfn ""
    printfn "[ C-style enum — turn_right(d: Direction) -> Direction ]"
    let mutable d = Direction.North
    for _ in 1..4 do
        let next = turn_right d
        printfn "  turn_right(%-6A) = %A" d next
        d <- next

    // ── 4. Tagged enum: ShapeFfi + area ───────────────────────────────────
    printfn ""
    printfn "[ tagged enum — area(shape: ShapeFfi) -> f64 ]"

    // Circle with radius 5
    let mutable circleData = ShapeCircleData()
    circleData.radius <- 5.0
    let mutable circleUnion = ShapeUnion()
    circleUnion.circle <- circleData
    let mutable circle = ShapeFfi()
    circle.tag  <- ShapeTag.Circle
    circle.data <- circleUnion
    printfn "  area(Circle    { radius=5 })      = %.4f" (area circle)  // π·25

    // Rectangle 6 × 4
    let mutable rectData = ShapeRectangleData()
    rectData.width  <- 6.0
    rectData.height <- 4.0
    let mutable rectUnion = ShapeUnion()
    rectUnion.rectangle <- rectData
    let mutable rect = ShapeFfi()
    rect.tag  <- ShapeTag.Rectangle
    rect.data <- rectUnion
    printfn "  area(Rectangle { width=6, height=4 }) = %.1f" (area rect)  // 24.0

    // ── 5. Strings: &str → String ─────────────────────────────────────────
    printfn ""
    printfn "[ strings — greet(name: &str) -> String ]"
    for name in [ "World"; "F#"; "FurstSharp" ] do
        let msg = withRustStr name (fun ptr len -> greet(ptr, len)) |> furstStrToString
        printfn "  greet(%12s) = %s" (sprintf "\"%s\"" name) msg

    // ── 6. Opaque handles: Counter (typed CounterHandle) ───────────────────
    printfn ""
    printfn "[ opaque handles — Counter (impl block → typed CounterHandle) ]"
    let c = counter_new 42L
    printfn "  counter_new(42)    → CounterHandle"
    printfn "  counter_get()      = %d" (counter_get c)
    counter_increment c
    counter_increment c
    counter_increment c
    printfn "  after 3 increments = %d" (counter_get c)   // 45
    counter_free c
    printfn "  counter_free()     ✓"

    printfn ""
    printfn "═══════════════════════════════════════"
    printfn "All examples completed successfully!"
    0
