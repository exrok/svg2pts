# svg2pts

[![](http://meritbadge.herokuapp.com/svg2pts) ![](https://img.shields.io/crates/d/svg2pts.svg)](https://crates.io/crates/svg2pts)
<!-- [![](https://docs.rs/svg2pts/badge.svg)](https://docs.rs/svg2pts/) -->

Command line utility to convert the paths in a SVG to a list of points. All paths in the SVG is converted into a list of points
curve interpolation is controlled by command line arguments. 

* [Installation](#installation)
* [Usage](#usage)
* [Examples](#usage)

<a name="Installation"></a>
## Installation

Using cargo: 
```sh
cargo install svg2pts
```

This will make the svg2pts binary available in your cargo binary directory; usually `~/.cargo/bin`.

## Usage

```text
svg2pts 0.1.0
Converts all paths in a svg to a list of points.

USAGE:
    svg2pts [OPTIONS] [ARGS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -a, --accuracy <accuracy>    Set target accuracy for bezier curve [default: 0.1]
    -d, --distance <distance>    Set target distance between points, use default units of SVG. If distance == 0.0
                                 (default), then the number points will be minimized while maintaining target accuracy
                                 [default: 0.0]

ARGS:
    <input>     Input SVG file, stdin if not present
    <output>    Output file, stdout if not present
```

## Examples

<p align="center">

### SVG converted to points with distance 1.5

<img
width="368"
src="https://raw.githubusercontent.com/exrok/svg2pts/master/media/plot1.png"
/>

### SVG converted to points with distance 3.5

<img
width="368"
src="https://raw.githubusercontent.com/exrok/svg2pts/master/media/plot2.png"
/>

### SVG converted to points without normalized distance

<img
width="368"
src="https://raw.githubusercontent.com/exrok/svg2pts/master/media/plot3.png"
/>

</p>

