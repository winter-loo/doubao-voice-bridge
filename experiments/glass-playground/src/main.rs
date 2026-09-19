#[cfg(windows)]
fn load_reference() -> Result<doubao_glass_live::PlaygroundReferenceImage, Box<dyn std::error::Error>> {
    let path=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/liquid-glass-reference/apple-liquid-glass-white-text-reference.webp");
    let rgba=image::open(&path)?.to_rgba8();
    let (width,height)=rgba.dimensions();
    let mut bgra=rgba.into_raw();
    for pixel in bgra.chunks_exact_mut(4) { pixel.swap(0,2); }
    Ok(doubao_glass_live::PlaygroundReferenceImage{bgra,width,height})
}

#[cfg(windows)]
fn make_contact_sheet(directory:&std::path::Path)->Result<(),Box<dyn std::error::Error>> {
    let names=[
        "source-white-black-text.png",
        "steady.png",
        "waveform.png",
        "text.png",
    ];
    let mut images=Vec::new();
    for name in names {
        images.push(image::open(directory.join(name))?.to_rgba8());
    }
    let width=images[0].width();
    let height=images[0].height();
    if !images.iter().all(|image|image.width()==width&&image.height()==height) {
        return Err("Deterministic check images disagree on geometry".into());
    }
    let mut sheet=image::RgbaImage::new(width*2,height*2);
    for (index,image) in images.iter().enumerate() {
        let ox=(index as u32%2)*width;
        let oy=(index as u32/2)*height;
        for (x,y,pixel) in image.enumerate_pixels() {
            sheet.put_pixel(ox+x,oy+y,*pixel);
        }
    }
    sheet.save(directory.join("contact-sheet.png"))?;
    Ok(())
}

#[cfg(windows)]
fn main() {
    let args:Vec<String>=std::env::args().skip(1).collect();
    let result:Result<(),Box<dyn std::error::Error>>=if args.iter().any(|arg|arg=="--check") {
        let output=args.iter()
            .find_map(|arg|arg.strip_prefix("--output=").map(std::path::PathBuf::from))
            .unwrap_or_else(||std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out"));
        doubao_glass_live::run_playground_check(&output)
            .and_then(|_|make_contact_sheet(&output))
            .map(|_|eprintln!("[glass-check] contact-sheet.png ready: {}",output.display()))
    } else if args.iter().any(|arg|arg=="--help") {
        println!("Liquid Glass playground\n\nInteractive:\n  cargo run --manifest-path experiments/glass-playground/Cargo.toml\n\nDeterministic L2 check:\n  cargo run --manifest-path experiments/glass-playground/Cargo.toml -- --check [--output=DIR]");
        Ok(())
    } else {
        let reference=load_reference().map_err(|error|{
            let message=format!(
                "Cannot decode canonical docs/liquid-glass-reference/apple-liquid-glass-white-text-reference.webp: {error}"
            );
            eprintln!("[glass-playground] {message}; Apple panel disabled");
            message
        });
        doubao_glass_live::run_playground(reference)
    };
    if let Err(error)=result {
        eprintln!("[glass-playground] ERROR: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("glass-playground requires Windows/D3D11");
    std::process::exit(2);
}
