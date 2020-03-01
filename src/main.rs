use lyon_geom::cubic_bezier::CubicBezierSegment;
use lyon_geom::euclid::Vector2D;
use std::fs::File;
use std::io::prelude::*;
use std::io::{self, BufWriter};
use kurbo::common::solve_quadratic; // usvg already uses kurbo
use usvg::prelude::*;
use usvg::{NodeKind, Options, PathSegment, Tree, TransformedPath};

type Pt = Vector2D<f64, lyon_geom::euclid::UnknownUnit>;

#[derive(Default, Debug)]
struct Opt {
    /// Set target distance between points, use default units of SVG.
    /// If distance == 0.0 (default), then the number points will be
    /// minimized while maintaining target accuracy.
    //    #[structopt(short = "d", long = "distance", default_value = "0.0")]
    distance: f64,

    /// Set target accuracy for bezier curve.
    //   #[structopt(short = "a", long = "accuracy", default_value = "0.1")]
    accuracy: f64,

    /// Input SVG file, stdin if not present
    //  #[structopt(parse(from_os_str))]
    input: Option<String>,

    /// Output file, stdout if not present
    // #[structopt(parse(from_os_str))]
    output: Option<String>,
}

fn print_usage() {
    println!(
        "{}",
        r#"svg2pts 0.1.2
Converts all paths in a svg to a list of points. Will ignore paths
with no stroke or fill. Output is a sequence of points, `X Y\n`. 

USAGE:
    svg2pts [OPTIONS] [ <input> [ <output> ] ]

FLAGS:
    -h, --help       Prints help information

OPTIONS:
    -a, --accuracy <accuracy>    Set target accuracy for bezier curve [default: 0.1]
    -d, --distance <distance>    Set target distance between points, depends on DPI of SVG.
                                 If distance == 0.0 point distance not normalized.
                                 [default: 0.0]

ARGS:
    <input>     Input SVG file, stdin if not present
    <output>    Output file, stdout if not present"#
    )
}

fn print_basic_usage() {
    println!(
        "{}",
        r#"
USAGE:
    svg2pts [OPTIONS] [ <input> [<output>] ]

For more information try --help
"#
    )
}

#[macro_export]
macro_rules! desc_err {
    (  $x:expr, {  $($y:expr),* } ) => { if let Some(x) = $x { x } else {
        eprint!("error: ");
        eprintln!($($y,)*);
        print_basic_usage();
        return None;
    }};
}
fn parse_args() -> Option<Opt> {
    let mut opts = Opt::default();
    opts.accuracy = 0.1;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        if arg.starts_with('-') {
            if arg == "-h" || arg == "--help" {
                print_usage();
                return None;
            } else if arg == "-d" || arg == "--distance" {
                let d = desc_err!(args.next(), {
                    "Missing argument after: {}", arg
                });
                let dist = desc_err!( d.parse::<f64>().ok(), {
                    "Invalid value for '{}' <f64>: invalid float literal", arg
                });
                if dist < 0.0 {
                    eprintln!("error: {} is out of range, distance >= 0", arg);
                    print_basic_usage();
                    return None;
                }
                opts.distance = dist;
            } else if arg == "-a" || arg == "--accuracy" {
                let a = desc_err!(args.next(), {
                    "Missing argument after: {}", arg
                });
                let acc = desc_err!( a.parse::<f64>().ok(), {
                    "Invalid value for '{}' <f64>: invalid float literal", arg
                });
                if acc <= 0.0 {
                    eprintln!("error: {} is out of range, accuracy > 0", arg);
                    print_basic_usage();
                    return None;
                }
                opts.accuracy = acc;
            } else {
                eprintln!("error: unknown flag {}", arg);
                print_usage();
                return None;
            }
        } else if opts.input.is_none() {
            opts.input = Some(arg);
        } else if opts.output.is_none() {
            opts.input = Some(arg)
        } else {
            eprintln!("error: unexpected extra argument {}", arg);
            print_usage();
            return None;
        }
    }

    Some(opts)
}

struct PathWriter<T: Write> {
    start: Pt,
    current: Pt,
    last: Pt,
    target_dist: f64,
    out: T,
    height: f64,
}

/// Finds if it exists the point on the line with distance, dist,
/// from point c.
fn pt_on_line_with_dist(dist: f64 ,c: Pt, line: (Pt,Pt)) -> Option<Pt> {
    let w = line.0 - c;
    let v = line.1 - line.0;
    let rq = solve_quadratic(
        w.x*w.x + w.y*w.y - dist*dist,
        2.0*(v.x*w.x + v.y*w.y),
        v.x*v.x + v.y*v.y,
    );
    let mut t_min = 2.0;
    for t in rq {
        if t >= -0.000001 && t <= 1.000001 && t < t_min {
            t_min = t;
        } 
    }
    if t_min <= 1.0 {
        Some(line.0.lerp(line.1, t_min))
    } else {
        None
    }
}

impl<T: Write> PathWriter<T> {
    fn new(out: T, target_dist: f64) -> PathWriter<T> {
        PathWriter {
            target_dist,
            start: Pt::default(),
            current: Pt::default(),
            last: Pt::default(),
            height: 0.0,
            out,
        }
    }

    fn write_pt(&mut self, pt: Pt) -> io::Result<()> {
        let mut buffer = ryu::Buffer::new();
        self.out.write_all(buffer.format(pt.x).as_bytes())?;
        self.out.write_all(&[b' '])?;
        self.out
            .write_all(buffer.format(self.height - pt.y).as_bytes())?;
        self.out.write_all(&[b'\n'])
    }

    fn move_to(&mut self, pt: Pt) -> io::Result<()> {
        self.start = pt;
        self.current = pt;
        self.last = pt;
        self.write_pt(pt)
    }

    /// Segments Line into distance lengthed segments
    fn line_to(&mut self, line_end: Pt) -> io::Result<()> {
        if self.target_dist == 0.0 {
            self.last = line_end; //record last
            return self.write_pt(line_end);
        }

        if let Some(pt) = pt_on_line_with_dist(self.target_dist, 
                                               self.current,
                                               (self.last, line_end)){
            self.current = pt;
            self.write_pt(self.current)?;
        } else { //shouldn't happen unless numerical floats point inaccuracy occours  
            self.current = self.last;
            self.write_pt(self.current)?;
        }

        self.last = line_end; //record last

        let line_dist = (self.current - line_end).length();
        if line_dist < self.target_dist {
            return Ok(()); //only ignore line if very close.
        }

        let td = self.target_dist / line_dist;

        let line_start = self.current;
        for i in 1.. {
            let t = (i as f64) * td;
            if t >= 1.0 {
                break;
            }
            self.current = line_start.lerp(line_end, t);
            self.write_pt(self.current)?;
        }
        Ok(())
    }

    fn close_path(&mut self) -> io::Result<()> {
        self.line_to(self.start)
    }
}

/// the deafult stdout is line-buffered causing considerable
/// overhead, on unix this is trival to work around.
#[cfg(target_family = "unix")]
fn raw_stdout() -> impl Write {
    use std::os::unix::io::FromRawFd;
    BufWriter::new(unsafe { File::from_raw_fd(1) })
}
#[cfg(not(target_family = "unix"))]
fn raw_stdout() -> impl Write {
    stdout() //sucks to be you
}

fn write_pts_from_paths<T: Write>(
    writer: &mut PathWriter<T>,
    acc: f64,
    paths: impl Iterator<Item = PathSegment>,
) -> io::Result<()> {
    for seg in paths {
        match seg {
            PathSegment::MoveTo { x, y } => {
                writer.move_to((x, y).into())?;
            }
            PathSegment::LineTo { x, y } => {
                writer.line_to((x, y).into())?;
            }
            PathSegment::ClosePath => {
                writer.close_path()?;
            }
            PathSegment::CurveTo { x1, y1, x2, y2, x, y } => {
                let bez = CubicBezierSegment {
                    from: (writer.last.x, writer.last.y).into(),
                    ctrl1: (x1, y1).into(),
                    ctrl2: (x2, y2).into(),
                    to: (x, y).into(),
                };
                for pt in bez.flattened(acc) {
                    writer.line_to(pt.to_vector())?;
                }
            }
        }
    }
    Ok(())
}

fn write_svg_pts<T: Write>(acc: f64, svg: &[u8], mut writer: PathWriter<T>) -> io::Result<()> {
    let tree = match Tree::from_data(svg, &Options::default()) {
        Ok(tree) => tree,
        Err(err) => {
            eprintln!("error: Unable to parse svg fatal error:\n\t {}", err);
            return Ok(());
        }
    };

    let height = tree.svg_node().view_box.rect.height();
    writer.height = height;
    for node in tree.root().descendants() {
        if let NodeKind::Path(ref path) = *node.borrow() {
            if path.fill.is_none() && path.stroke.is_none() {
                continue;
            }
            for subpaths in path.data.subpaths() {
                write_pts_from_paths(&mut writer, acc,
                  TransformedPath::new(&subpaths, node.transform()))?;
            }
        }
    }

    Ok(())
}


fn main() -> std::io::Result<()> {
    let opt = if let Some(opt) = parse_args() {
        opt
    } else {
        return Ok(());
    };

    let mut svg_buf = Vec::default();

    if let Some(ref filename) = opt.input {
        let mut file = match File::open(filename) {
            Ok(file) => file,
            Err(err) => {
                eprintln!(
                    "error: could not read input file, `{}`:\n\t {}",
                    filename, err
                );
                return Ok(());
            }
        };
        file.read_to_end(&mut svg_buf)?;
    } else {
        std::io::stdin().read_to_end(&mut svg_buf)?;
    }

    if let Some(ref filename) = opt.output {
        let file = match File::create(filename) {
            Ok(file) => file,
            Err(err) => {
                eprintln!(
                    "error: could not create output file, `{}`:\n\t {}",
                    filename, err
                );
                return Ok(());
            }
        };
        let writer = PathWriter::new(BufWriter::new(file), opt.distance);
        write_svg_pts(opt.accuracy, &svg_buf, writer)?;
    } else {
        let writer = PathWriter::new(raw_stdout(), opt.distance);
        write_svg_pts(opt.accuracy, &svg_buf, writer)?;
    }

    Ok(())
}
