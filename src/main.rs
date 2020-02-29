use lyon_geom::cubic_bezier::CubicBezierSegment;
use lyon_geom::euclid::Vector2D;
use std::fs::File;
use std::io::prelude::*;
use std::io::{self, BufWriter};
use usvg::{NodeKind, Options, PathSegment, Tree};

use std::path::PathBuf;
use structopt::StructOpt;

type Pt = Vector2D<f64, lyon_geom::euclid::UnknownUnit>;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "svg2pts",
    about = "Converts all paths in a svg to a list of points."
)]
struct Opt {
    /// Set target distance between points, use default units of SVG.
    /// If distance == 0.0 (default), then the number points will be
    /// minimized while maintaining target accuracy.
    #[structopt(short = "d", long = "distance", default_value = "0.0")]
    distance: f64,

    /// Set target accuracy for bezier curve.
    #[structopt(short = "a", long = "accuracy", default_value = "0.1")]
    accuracy: f64,

    /// Input SVG file, stdin if not present
    #[structopt(parse(from_os_str))]
    input: Option<PathBuf>,

    /// Output file, stdout if not present
    #[structopt(parse(from_os_str))]
    output: Option<PathBuf>,
}

struct PathWriter<T: Write> {
    start: Pt,
    current: Pt,
    target_dist: f64,
    out: T,
    height: f64,
}

impl<T: Write> PathWriter<T> {
    fn new(out: T, target_dist: f64) -> PathWriter<T> {
        PathWriter {
            target_dist,
            start: Pt::default(),
            current: Pt::default(),
            height: 0.0,
            out,
        }
    }

    fn write_pt(&mut self, pt: Pt) -> io::Result<()> {
        let mut buffer = ryu::Buffer::new();
        self.out.write_all(buffer.format(pt.x).as_bytes())?;
        self.out.write_all(&[b' '])?;
        self.out.write_all(buffer.format(self.height-pt.y).as_bytes())?;
        self.out.write_all(&[b'\n'])
    }

    fn move_to(&mut self, pt: Pt) -> io::Result<()> {
        self.start = pt;
        self.current = pt;
        self.write_pt(pt)
    }

    /// Segments Line into distance lengthed segments
    fn line_to(&mut self, line_end: Pt) -> io::Result<()> {
        if self.target_dist == 0.0 {
            return self.write_pt(line_end);
        }

        let line_dist = (self.current - line_end).length();

        if line_dist < self.target_dist {
            return Ok(());
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
    paths: &[PathSegment],
) -> io::Result<()> {
    let mut last_point = (0f64, 0f64);
    for seg in paths {
        match seg {
            &PathSegment::MoveTo { x, y } => {
                last_point = (x, y);
                writer.move_to((x, y).into())?;
            }
            &PathSegment::LineTo { x, y } => {
                last_point = (x, y);
                writer.line_to((x, y).into())?;
            }
            &PathSegment::ClosePath => {
                writer.close_path()?;
            }
            &PathSegment::CurveTo { x1, y1, x2, y2, x, y } => {
                let bez = CubicBezierSegment {
                    from: last_point.into(),
                    ctrl1: (x1, y1).into(),
                    ctrl2: (x2, y2).into(),
                    to: (x, y).into(),
                };
                for pt in bez.flattened(acc) {
                    writer.line_to(pt.to_vector())?;
                    last_point = (pt.x, pt.y);
                }
            }
        }
    }
    Ok(())
}

fn write_svg_pts<T: Write>(acc: f64, svg: &[u8], mut writer: PathWriter<T>) -> io::Result<()> {
    let tree = Tree::from_data(svg, &Options::default()).unwrap();
    let height = tree.svg_node().view_box.rect.height();
    writer.height = height;
    for node in tree.root().descendants() {
        if let NodeKind::Path(ref path) = *node.borrow() {
            for i in path.data.subpaths() {
                write_pts_from_paths(&mut writer, acc, &i)?;
            }
        }
    }

    Ok(())
}

fn main() -> std::io::Result<()> {
    let opt = Opt::from_args();

    let mut svg_buf = Vec::default();

    if let Some(filename) = opt.input {
        File::open(filename)?.read_to_end(&mut svg_buf)?;
    } else {
        std::io::stdin().read_to_end(&mut svg_buf)?;
    }

    if let Some(filename) = opt.output {
        let writer = PathWriter::new(BufWriter::new(File::create(filename)?), opt.distance);
        write_svg_pts(opt.accuracy, &svg_buf, writer)?;
    } else {
        let writer = PathWriter::new(raw_stdout(), opt.distance);
        write_svg_pts(opt.accuracy, &svg_buf, writer)?;
    }

    Ok(())
}
