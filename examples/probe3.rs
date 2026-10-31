fn main() {
    let src = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    match bibsmith::parse_lenient(&src) {
        Ok(b) => println!("parsed {} entries", b.iter().count()),
        Err(e) => {
            let line = src[..e.span.start.min(src.len())].bytes().filter(|b| *b == b'\n').count() + 1;
            println!("error {:?} at bytes {:?} line {}", e.kind, e.span, line);
            println!("line text: {:?}", src.lines().nth(line - 1));
            match bibsmith::normalize::repair_value_line(&src, line) {
                Some(fixed) => {
                    println!("repair ok, new line: {:?}", fixed.lines().nth(line - 1));
                    match bibsmith::parse_lenient(&fixed) {
                        Ok(b) => println!("after repair: {} entries", b.iter().count()),
                        Err(e2) => {
                            let l2 = fixed[..e2.span.start.min(fixed.len())].bytes().filter(|b| *b == b'\n').count() + 1;
                            println!("still: {:?} line {}", e2.kind, l2);
                        }
                    }
                }
                None => println!("no repair"),
            }
        }
    }
}
