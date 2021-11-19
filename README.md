# LDraw.rs

[LDraw] is an open standard for virtual LEGO CAD.

LDraw.rs is a library for manipulating and rendering LDraw model files. Built with [Rust] language, it can be compiled to [WebAssembly] and run it in your web browser directly.

Currently, it is comprised of following crates:

* `ldraw` for basic I/O and structuring of LDraw files.
* `ir` (abbr, Internal representation) for providing higher level concepts beyond what LDraw can provide. Also, ir can be used for processing part data to be friendly with modern graphics pipeline.
* `renderer` for rendering model with [OpenGL]/WebGL.
* `olr` (abbr, Offline renderer) for rendering model offscreen.

LDraw.rs is a part of a project which aims to create a web-based LEGO CAD service.

## Examples

You can see a simple model viewer in action on your web browser:

* [Car model](https://segfault87.github.io/ldraw-rs-preview/#models/car.ldr) (from official LDraw samples)
* [Pyramid model](https://segfault87.github.io/ldraw-rs-preview/#models/pyramid.ldr) (from official LDraw samples)
* [6973 Deep Freeze Defender](https://segfault87.github.io/ldraw-rs-preview/#models/6973.ldr)

## License

This project is licensed under of MIT license ([LICENSE.md](LICENSE.md) or http://opensource.org/licenses/MIT).

## Trademarks

LDraw is a trademark of the Estate of James Jessiman. LEGO is a registered trademark of the LEGO Group.

  [LDraw]: http://www.ldraw.org
  [OpenGL]: https://www.opengl.org
  [Rust]: https://www.rust-lang.org
  [WebAssembly]: https://webassembly.org