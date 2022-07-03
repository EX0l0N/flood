use std::{env, fs::File, io, sync::Arc, sync::RwLock, thread};

struct Image {
    data: Vec<u8>,
    width: u32,
    height: u32,
}

#[derive(Debug)]
struct Color {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[derive(Debug, Copy, Clone)]
struct Color16 {
    r: u16,
    g: u16,
    b: u16,
    a: u16,
}

#[derive(Debug)]
struct Pixel(i64, i64);

#[derive(Debug)]
struct Change {
    loc: Pixel,
    col: Color,
}

impl Image {
    fn get_kernel(&self, p: &Pixel) -> Vec<Option<Color16>> {
        let x64 = p.0 - 1;
        let y64 = p.1 - 1;

        let mut idx = 0;
        let mut kern = vec![None; 9];

        for y in y64..y64 + 3 {
            for x in x64..x64 + 3 {
                if x < 0 || y < 0 {
                    kern[idx] = None;
                    idx += 1;
                    continue;
                }

                let xu = x as u32;
                let yu = y as u32;

                if xu >= self.width || yu >= self.height {
                    kern[idx] = None;
                    idx += 1;
                    continue;
                }

                let pos = (xu * 4 + self.width * 4 * yu) as usize;
                kern[idx] = Some(Color16 {
                    r: self.data[pos] as u16,
                    g: self.data[pos + 1] as u16,
                    b: self.data[pos + 2] as u16,
                    a: self.data[pos + 3] as u16,
                });
                idx += 1;
            }
        }

        kern
    }

    fn test_pixel(&self, p: &Pixel) -> bool {
        let x = p.0 as u32;
        let y = p.1 as u32;

        return self.data[(x * 4 + self.width * 4 * y + 3) as usize] == 255;
    }

    fn set_pixel(&mut self, p: &Pixel, c: &Color) {
        let x = p.0 as u32;
        let y = p.1 as u32;

        let pos = (x * 4u32 + self.width * 4u32 * y) as usize;

        self.data[pos] = c.r;
        self.data[pos + 1] = c.g;
        self.data[pos + 2] = c.b;
        self.data[pos + 3] = c.a;
    }
}

fn load_image(path: &String) -> io::Result<Image> {
    let mut decoder = png::Decoder::new(File::open(path)?);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut img_data = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut img_data)?;

    if info.color_type != png::ColorType::Rgba {
        panic!("Must have RGBA!");
    }

    Ok(Image {
        data: img_data,
        width: info.width,
        height: info.height,
    })
}

fn save_image(path: &String, arg: &RwLock<Image>) {
    let file = File::create(path).unwrap();
    let ref mut w = io::BufWriter::new(file);

    let img = arg.read().unwrap();

    let mut encoder = png::Encoder::new(w, img.width, img.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder.write_header().unwrap();

    writer.write_image_data(&img.data).unwrap();
}

fn analyze_step(arg: &Arc<RwLock<Image>>, sz: &mut [usize; 4]) -> Vec<Vec<Change>> {
    let height;
    let width;

    {
        let img = arg.read().unwrap();

        height = img.height as i64;
        width = img.width as i64
    }

    let hafheight = height / 2;
    let hafwidth = width / 2;
    let mut threads = Vec::with_capacity(4);

    for j in 0i64..=1 {
        for i in 0i64..=1 {
            let rwimg = arg.clone();
            let vz = sz[(i + j * 2) as usize];

            threads.push(thread::spawn(move || {
                let mut v = Vec::with_capacity(vz);
                let img = rwimg.read().unwrap();

                for y in j * hafheight..(j + 1) * hafheight {
                    for x in i * hafwidth..(i + 1) * hafwidth {
                        let p = Pixel(x, y);

                        if img.test_pixel(&p) {
                            continue;
                        }

                        let kern = img.get_kernel(&p);

                        let mut hits = 0u16;
                        let mut r = 0u16;
                        let mut g = 0u16;
                        let mut b = 0u16;

                        for v in 0..=2 as i64 {
                            for u in 0..=2 as i64 {
                                if u == 1 && v == 1 {
                                    continue;
                                }

                                let color = &kern[(u + v * 3) as usize];

                                match color {
                                    Some(c) => {
                                        if c.a == 255 {
                                            r += c.r;
                                            g += c.g;
                                            b += c.b;
                                            hits += 1;
                                        }
                                    }
                                    None => {}
                                }
                            }
                        }
                        if hits < 3 {
                            continue;
                        }
                        r /= hits;
                        g /= hits;
                        b /= hits;
                        //a /= hits;

                        v.push(Change {
                            loc: Pixel(x, y),
                            col: Color {
                                r: r as u8,
                                g: g as u8,
                                b: b as u8,
                                a: 255,
                            },
                        });
                    }
                }
                v
            }));
        }
    }

    let mut vecs = Vec::with_capacity(threads.len());

    for tidx in (0..threads.len()).rev() {
        let v = threads.pop().unwrap().join().unwrap();
        sz[tidx] = v.len();
        vecs.push(v);
    }

    vecs
}

fn apply_changes(arg: &RwLock<Image>, vecs: &Vec<Vec<Change>>) {
    let mut img = arg.write().unwrap();

    for v in vecs.iter() {
        for chng in v.iter() {
            img.set_pixel(&chng.loc, &chng.col);
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        panic!("Must specify files.");
    }

    let img = match load_image(&args[1]) {
        Ok(img) => Arc::new(RwLock::new(img)),
        Err(e) => panic!("Problem opening the file: {:?}", e),
    };

    let mut v_size = [256usize; 4];

    loop {
        let v = analyze_step(&img, &mut v_size);
        let vlen = v_size[0] + v_size[1] + v_size[2] + v_size[3];
        println!("Len {}", vlen);
        if vlen == 0 {
            break;
        }
        apply_changes(&img, &v);
    }

    save_image(&args[2], &img);
}
