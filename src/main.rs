use lyon_geom::cubic_bezier::CubicBezierSegment;
use lyon_geom::euclid::Vector2D;
use std::fs::File;
use std::io::prelude::*;
use std::io;
use usvg::prelude::*;
use kurbo::common::solve_quadratic; // usvg already uses kurbo
use usvg::{NodeKind, Options, PathSegment, Tree, TransformedPath};

use error_chain::bail;
mod errors {
    use error_chain::error_chain;
    error_chain! {
    }
}
use errors::*;
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
    accuracy: Option<f64>,

    points: u64,

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
        r#"svg2pts 0.1.5
Converts all paths in a svg to a list of points. Will ignore paths
with no stroke or fill. Output is a sequence of points, `X Y\n`. 

USAGE:
    svg2pts [OPTIONS] [ <input> [ <output> ] ]

FLAGS:
    -h, --help       Prints help information

OPTIONS:
    -a, --accuracy <accuracy>    Set tolerance threshold for bezier curve approximation, 
                                 lower -> higher quality
                                 [default: 0.0005]

    -d, --distance <distance>    Set Target distance between points, depends on DPI of SVG.
                                 If distance == 0.0 point distance not normalized.
                                 [default: 0.0]

    -p, --points   <points>      Calculate target distance to generate approximatly <points> 
                                 number of points.
                                 [default: 0]

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

fn parse_args() -> Result<Opt> {
    let mut opts = Opt::default();
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        if arg.starts_with('-') {
            if arg == "-h" || arg == "--help" {
                print_usage();
                ::std::process::exit(0);
            } else if arg == "-d" || arg == "--distance" {
                let d = args.next().chain_err(|| {
                    format!("Missing argument after: {}", arg)
                })?; 

                let dist = d.parse::<f64>().chain_err(|| {
                    format!("Invalid value '{}' <f64>", arg)
                })?;

                if dist < 0.0 {
                    bail!("{} is out of range, distance >= 0", arg);
                }

                opts.distance = dist;
            } else if arg == "-p" || arg == "--points" {
                let p = args.next().chain_err(|| {
                    format!("Missing argument after: {}", arg)
                })?; 

                let pts = p.parse::<u64>().chain_err(|| {
                    format!("Invalid value '{}' <u64>", arg)
                })?;

                opts.points = pts;

            } else if arg == "-a" || arg == "--accuracy" {
                let a = args.next().chain_err(|| {
                    format!("Missing argument after: {}", arg)
                })?;

                let acc = a.parse::<f64>().chain_err(|| {
                    format!("Invalid value '{}' <f64>", arg)
                })?;

                if acc <= 0.0 {
                    bail!("{} is out of range, accuracy >= 0", arg);
                }
                opts.accuracy = Some(acc);
            } else {
                print_basic_usage();
                bail!("unknown flag {}", arg);
            }
        } else if opts.input.is_none() {
            opts.input = Some(arg);
        } else if opts.output.is_none() {
            opts.output = Some(arg)
        } else {
            bail!("unexpected extra argument {}", arg);
        }
    }

    Ok(opts)
}


struct PathWriter {
    start: Pt,
    current: Pt,
    last: Pt,
    accuracy: f64,
    target_dist: f64,
    out: PointBufWriter,
    height: f64,
}

impl PathWriter {
    fn new(out: PointBufWriter, target_dist: f64, accuracy: f64, height: f64) -> PathWriter {
        PathWriter {
            target_dist,
            start: Pt::default(),
            current: Pt::default(),
            last: Pt::default(),
            accuracy,
            height,
            out,
        }
    }

    fn write_pt(&mut self, pt: Pt) -> io::Result<()> {
        self.out.write(pt.x, self.height - pt.y)
    }

    fn move_to(&mut self, pt: Pt) -> io::Result<()> {
        self.start = pt;
        self.current = pt;
        self.last = pt;
        self.write_pt(pt)
    }

    fn write_path(&mut self, path: impl Iterator<Item = PathSegment>) -> io::Result<()> {
        use PathSegment::*;
        for seg in path {
            match seg {
                MoveTo { x, y } => {
                    self.move_to((x, y).into())?;
                }
                LineTo { x, y } => {
                    self.line_to((x, y).into())?;
                }
                ClosePath => {
                    self.close_path()?;
                }
                CurveTo { x1, y1, x2, y2, x, y } => {
                    let bez = CubicBezierSegment {
                        from: (self.last.x, self.last.y).into(),
                        ctrl1: (x1, y1).into(),
                        ctrl2: (x2, y2).into(),
                        to: (x, y).into(),
                    };
                    for pt in bez.flattened(self.accuracy) {
                        self.line_to(pt.to_vector())?;
                    }
                }
            }
        }
        Ok(())
    }
    /// Segments Line into distance lengthed segments
    fn line_to(&mut self, line_end: Pt) -> io::Result<()> {
        if self.target_dist == 0.0 {
            self.last = line_end; //record last
            return self.write_pt(line_end);
        }

        // Find point on line (self.last, line_end) such that is
        // target_dist away from self.current
        let w = line_end - self.current;
        let v = self.last - line_end;
        let c = w.square_length() - self.target_dist* self.target_dist;
        if c < 0.0 { // line_end is two close 
            self.last = line_end; 
            return Ok(());
        }

        let intersect = solve_quadratic(
            c,
            2.0*(v.dot(w)),
            v.square_length()
        );

        let mut t_min = 2.0;
        for t in intersect {
            if t >= -0.000001 && t <= 1.000001 && t < t_min {
                t_min = t;
            } 
        }

        if t_min <= 1.0 {
            self.current = line_end.lerp(self.last, t_min);
            self.write_pt(self.current)?;
        } else {
            // line_end is two close; shouldn't happen since we
            // checked above we have extended bounds on t
            // to account for numerical imprecision 
            self.last = line_end; 
            return Ok(());
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
    use std::os::unix::io::{AsRawFd, FromRawFd};
    let stdout = AsRawFd::as_raw_fd(&io::stdout());
    let stdout: File = unsafe {
        FromRawFd::from_raw_fd(stdout)
    };
    stdout
}

#[cfg(not(target_family = "unix"))]
fn raw_stdout() -> impl Write {
    stdout() //sucks to be you
}

fn path_distance(
    acc: f64,
    paths: impl Iterator<Item = PathSegment>,
) -> f64 {
    use PathSegment::*;
    let mut last = (0.0,0.0);
    let mut start = (0.0,0.0);
    let mut dist = 0.0;
    for seg in paths {
        match seg {
            MoveTo { x, y } => {
                last = (x,y);
                start = last;
            }
            LineTo { x, y } => {
                dist += (Pt::new(x, y) - Pt::from(last)).length();
                last = (x,y);
            }
            ClosePath => {
                dist += (Pt::from(start) - Pt::from(last)).length();
            }
            CurveTo { x1, y1, x2, y2, x, y } => {
                let bez = CubicBezierSegment {
                    from: last.into(),
                    ctrl1: (x1, y1).into(),
                    ctrl2: (x2, y2).into(),
                    to: (x, y).into(),
                };
                dist += bez.approximate_length(acc);
                last = (x,y);
            }
        }
    }
    dist
}

use usvg::PathData;
use usvg::Transform;
use std::rc::Rc;

fn extract_paths(svg: &Tree) -> Vec<(Rc<PathData>, Transform)> {
    let mut paths = Vec::default();
    for node in svg.root().descendants() {
        if let NodeKind::Path(ref path) = *node.borrow() {
            if path.fill.is_some() || path.stroke.is_some() {
                paths.push((path.data.clone(), node.transform()));
            }
        }
    }
    paths
}

/// Point Buffer writer for zero copy float writing
/// Improves performance 20% over the version without
/// unsafe
const BUFFER_SIZE:usize = 4*4096; //16KB 
struct PointBufWriter {
    buf: Box<[u8; BUFFER_SIZE]>,
    out: Box<dyn Write>,
    pos: usize,
}

impl PointBufWriter {
    fn new(writer: Box<dyn Write>) -> PointBufWriter {
        PointBufWriter{
            buf: Box::new([0; BUFFER_SIZE]),
            out: writer,
            pos: 0,
        }
    }

    fn write(&mut self, x: f64, y: f64) -> io::Result<()> {
        use ryu::raw::format64;
        if (self.pos + 51) >= BUFFER_SIZE { //ENSURE atleast 51 bytes free.
            self.out.write_all(&self.buf[..self.pos])?;
            self.pos = 0;
        }
        let buf = self.buf.as_mut_ptr() as *mut u8;
        let mut pos = self.pos as isize;
        unsafe {
            // Format64 need 24 bytes each to writes to be safe
            // the two char writes use 2 more bytes
            // the total written is 50 bytes at maximum
            // The above check ensures there always enough room.
            pos += format64(x, buf.offset(pos)) as isize;
            *buf.offset(pos) = b' ';
            pos += 1;
            pos += format64(y, buf.offset(pos)) as isize;
            *buf.offset(pos) = b'\n';
            pos += 1;
        }
        self.pos = pos as usize;
        Ok(())
    }
}

impl Drop for PointBufWriter {
    fn drop(&mut self) {
        if self.pos > 0 {
            self.out.write_all(&self.buf[..self.pos]).ok();
            self.pos = 0;
        }
    }
}


fn run() -> Result<()> {
    let opt = parse_args()?;

    let mut svg_buf = Vec::default();

    if let Some(ref filename) = opt.input {
        File::open(filename)
            .chain_err(|| "Failed to open input")?
            .read_to_end(&mut svg_buf)
            .chain_err(|| "Failed to read input")?;
    } else {
        std::io::stdin().read_to_end(&mut svg_buf)
            .chain_err(|| "Failed reading from stdin")?;
    }

    let pt_writer = if let Some(ref filename) = opt.output {
        PointBufWriter::new(Box::new(File::create(filename)
                                     .chain_err(|| "Failed to open output")?))
    } else {
        PointBufWriter::new(Box::new(raw_stdout()))
    };

    let tree = Tree::from_data(&svg_buf, &Options::default())
        .chain_err(|| "unable to parse svg")?;

    let paths = extract_paths(&tree);

    let height = tree.svg_node().view_box.rect.height();

    let distance = if opt.points > 0 {
        let path_distance:f64 = paths.iter().map(|(path, transform)| path_distance(
            0.05, TransformedPath::new(path, *transform)
        )).sum();
        path_distance / (opt.points as f64) 
    } else {
        opt.distance
    };

    let accuracy = opt.accuracy.unwrap_or(if distance == 0.0 {
        0.05
    } else {
        distance / 25.0
    });
    let mut writer = PathWriter::new(pt_writer, distance, accuracy, height);

    for (path, transform) in &paths {
        writer.write_path(TransformedPath::new(path, *transform))
            .chain_err(|| "failed writing points")?;
    }

    Ok(())
}

fn main() {
    if let Err(ref e) = run() {
        eprint!("error: {}: ", e);

        for e in e.iter().skip(1) {
            eprintln!("{}", e);
        }

        std::process::exit(1);
    }
}
