use tempfile::NamedTempFile;
use std::io::{Read, Seek, SeekFrom};
use assert_cmd::Command;
use predicates::prelude::*;
use lazy_static::lazy_static;
use lyon_geom::euclid::Vector2D;
type Pt = Vector2D<f64, lyon_geom::euclid::UnknownUnit>;

type Res<I> = Result<I, Box<dyn std::error::Error + Send + Sync>>;
fn extract_pts(input: &str) -> Res<Vec<Pt>> {
    let mut vec:Vec<Pt> = Vec::with_capacity(256);
    for line in input.lines() {
        let mut nums = line.split(' ');
        vec.push(
            (nums.next().ok_or("Expected Point Value")?.parse::<f64>()?,
             nums.next().ok_or("Expected Point Value")?.parse::<f64>()?)
                .into());
    }
    Ok(vec)
}

fn contains_path(dist: f64, tol: f64, pts: &[Pt], path: &[Pt]) -> bool {
    if pts.len() == 0 { return true; }
    let mut lines = path.windows(2);
    let mut pos = 0;
    while let Some(&[a, b]) = lines.next() {
        let det = Pt::new(b. y -a.y, -b.x + a.x);
        let c = b.x*a.y - b.y*a.x;
        let line_len = (a-b).length();
        let tol_len = (1.0 + tol)*((a-b).length() + dist/4.0);
        let tol_len_sq = tol_len*tol_len;
        let is_near_line = |pt:Pt| {
            (pt.dot(det) + c).abs()/line_len < dist &&
                (pt - a).square_length() < tol_len_sq &&
                (pt - b).square_length() < tol_len_sq 
        };
        while is_near_line(pts[pos]) {
            pos += 1;
            if pos >= pts.len() {
                return true;
            }
        }
    } 
    let k = ((pts.len()-pos) as f64)/(pts.len() as f64);
    k < tol
}

// enum PathAssertion {
//     Distance{tolerance: f64, target: f64},
//     Points(Range<usize>),
//     Path{tolerance: f64, distance: f64, target_path: &'static [Pt]},
// }
// impl PathAssertion {
//     fn assert(&self ,path: &[Pt]) {
//         use PathAssertion::*;
//         match self {
//             &Distance{tolerance, target} => {
//                 assert!(check_distance(tolerance, target, path))
//             },
//             Points(tol_range) => {
//                 assert_range!(tol_range.clone(), path.len());
//             },
//             &Path{tolerance, distance, target_path} => {
//                 assert!(
//                     contains_path(distance, tolerance, path, target_path) &&
//                         contains_path(distance, tolerance, target_path, path) 
//                 );
//             },
//         }
//     }
// }

fn same_path(dist: f64, tol: f64, path1: &[Pt], path2: &[Pt]) -> bool {
    contains_path(dist, tol, path1, path2) &&
    contains_path(dist, tol, path2, path1) 
}

use std::ops::Range;
fn in_range<T: std::cmp::PartialOrd>(range: &std::ops::Range<T>, value: T) -> (&std::ops::Range<T>,
                                                                              Result<T,T>) {
    if range.contains(&value) {
        (range, Ok(value))
    } else {
        (range, Err(value))
    }
}

#[macro_export]
macro_rules! assert_lt {
    ( $a:expr , $b:expr) => {{
        if !($a < $b) {
            panic!("assert!({} < {}): ({} < {})",$a,$b, std::stringify!($a), std::stringify!($b));
        }
    }};
}
#[macro_export]
macro_rules! assert_range {
    ( $r:expr , $x:expr) => {{
        assert_eq!(in_range(&($r), $x), (&$r, Ok($x)));
    }};
}
lazy_static! {
    static ref DATA_SVG1_PTS: Vec<Pt> = extract_pts(include_str!("data/output_complex_a0.01.pts")).unwrap();
    static ref DATA_SVG2_PTS: Vec<Pt> = extract_pts(include_str!("data/output_logo_d1.5.pts")).unwrap();
}
static DATA_SVG1: &'static str = include_str!("data/complex.svg");
static DATA_SVG1_PATH: &'static str = "tests/data/complex.svg";
static DATA_SVG2_PATH: &'static str = "tests/data/logo.svg";

fn check_pts(dist: f64, tol:f64, args: &[ &str ], output: &[Pt],  tol_range: Range<usize>) {
    let mut cmd = Command::cargo_bin("svg2pts").unwrap();
    let assert = cmd.args(args).assert();
    assert.stdout(predicate::function(|out: &str| {
        if let Ok(p) = extract_pts(out) {
            assert_range!(tol_range, p.len());
            assert!(same_path(dist, tol, &p, output));
            true
        } else {
            false
        }
    })).success();
}
fn check_distance(dist:f64, tol:f64, path: &[Pt]) -> bool {
    let mut lines = path.windows(2);
    let mut matching = 0;
    let range = ((1.0-0.25*tol)*dist)..((1.0+0.25*tol)*dist);
    while let Some(&[a,b]) = lines.next() {
        let dist = (a-b).length();
        if range.contains(&dist) {
            matching += 1;
        }
    }
    let k = ((path.len()-matching) as f64)/(path.len() as f64);
    k < tol
}

fn check_dist(dist: f64, tol:f64, args: &[ &str ], output: &[Pt],  dist_target: f64) {
    let mut cmd = Command::cargo_bin("svg2pts").unwrap();
    let assert = cmd.args(args).assert();
    assert.stdout(predicate::function(|out: &str| {
        if let Ok(p) = extract_pts(out) {
            assert!(check_distance(dist_target, tol, &p));
            assert!(same_path(dist, tol, &p, output));
            true
        } else {
            false
        }
    })).success();
}

#[test]
fn output_file_svg() {
    let pts = &DATA_SVG1_PTS;
    let mut tmpfile = NamedTempFile::new().unwrap();
    let mut cmd = Command::cargo_bin("svg2pts").unwrap();
    let assert = cmd.args(&["-d", "0.8", DATA_SVG1_PATH, tmpfile.path().to_str().unwrap()]).assert();
    tmpfile.seek(SeekFrom::Start(0)).unwrap();
    let mut buf = String::new();
    tmpfile.read_to_string(&mut buf).unwrap();
    let gen_pts = extract_pts(&buf).unwrap();
    
    assert!(same_path(4.0, 0.05, &pts, &gen_pts));
    assert.success();
    
}

#[test]
fn arg_help() {
    let mut cmd = Command::cargo_bin("svg2pts").unwrap();
    let assert = cmd.args(&["-h"]).assert();
    assert.stdout(predicate::function(|out: &str| {
        out.contains("USAGE")
    })).success();

    let mut cmd = Command::cargo_bin("svg2pts").unwrap();
    let assert = cmd.args(&["--help"]).assert();
    assert.stdout(predicate::function(|out: &str| {
        out.contains("USAGE")
    })).success();
}

#[test]
fn pipe_input_svg() {
    let pts = &DATA_SVG1_PTS;
    let mut cmd = Command::cargo_bin("svg2pts").unwrap();
    let assert = cmd.args(&["-d", "0.8"]).write_stdin(DATA_SVG1).assert();
    assert.stdout(predicate::function(|out: &str| {
        let p = extract_pts(out).unwrap();
        assert!(same_path(4.0, 0.05, &p, &pts));
         true
    })).success();
}
#[test]
fn distance_target_with_logo_svg() {
    let pts = &DATA_SVG2_PTS;
    check_dist(1.5, 0.05, &["-d", "0.25", "tests/data/logo.svg"], &pts, 0.25);
    check_dist(2.0, 0.05, &["-d", "0.8", "tests/data/logo.svg"], &pts, 0.8);
    check_dist(4.0, 0.05, &["-d", "4.0", "tests/data/logo.svg"], &pts, 4.0);
}

#[test]
fn distance_target_with_complex_svg() {
    let pts = &DATA_SVG1_PTS;
    check_dist(3.0, 0.03, &["-d", "0.3", "tests/data/complex.svg"], &pts, 0.3);
    check_dist(3.0, 0.08, &["--distance", "0.8", "tests/data/complex.svg"], &pts, 0.8);
    check_dist(3.0, 0.08, &["--distance", "0.8","-a", "1.00", "tests/data/complex.svg"], &pts, 0.8);
    check_dist(3.0, 0.08, &["--distance", "0.8","-a", "0.001", "tests/data/complex.svg"], &pts, 0.8);
    check_dist(3.0, 0.08, &["--distance", "4.0", "tests/data/complex.svg"], &pts, 4.0);
}

#[test]
fn points_target_with_logo_svg() {
    let pts = &DATA_SVG2_PTS;
    check_pts(10.0, 0.10, &["-p", "200", DATA_SVG2_PATH], &pts, 150..250);
    check_pts(5.0, 0.08, &["--points", "500", DATA_SVG2_PATH], &pts, 490..600);
    check_pts(3.0, 0.08, &["-p", "2000", DATA_SVG2_PATH], &pts, 1900..2100);
}

#[test]
fn points_target_with_complex_svg() {
    let pts = &DATA_SVG1_PTS;
    check_pts(3.0, 0.08, &["-p", "200", "tests/data/complex.svg"], &pts, 150..250);
    check_pts(2.0, 0.08, &["-p", "500", "tests/data/complex.svg"], &pts, 400..600);
    check_pts(0.1, 0.01, &["-p", "2000","tests/data/complex.svg"], &pts, 1900..2100);
}

