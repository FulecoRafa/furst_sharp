module Program

open FurstBindings

[<EntryPoint>]
let main _argv =
    printfn "FurstSharp example — calling Rust fibonacci via P/Invoke"
    printfn ""

    let inputs = [ 0L; 1L; 5L; 10L; 20L ]
    for n in inputs do
        let result = fibonacci n
        printfn "  fibonacci(%d) = %d" n result

    printfn ""
    printfn "Success!"
    0
