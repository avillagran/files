#!/usr/bin/env python3
"""Genera la tabla propia desde grupos explícitos mantenidos en este archivo."""
from pathlib import Path
# Datos propios; sin cargar bases MIME ni inspeccionar contenido en ejecución.
groups = {
 'text/plain': 'txt text log conf cfg ini env gitignore gitattributes editorconfig license authors readme nfo asc lst properties desktop service socket timer target mount automount path rules udev inputrc bashrc zshrc profile npmrc yarnrc',
 'text/markdown':'md markdown mdown mkd', 'text/csv':'csv tsv', 'text/html':'html htm shtml xhtml',
 'text/css':'css scss sass less', 'text/javascript':'js mjs cjs jsx', 'application/typescript':'ts tsx cts',
 'application/json':'json jsonc jsonl map ipynb', 'application/toml':'toml lock', 'application/yaml':'yaml yml',
 'application/xml':'xml xsl xslt xsd dtd rss atom opml plist svgz',
 'text/x-python':'py pyw pyi pyx pxd', 'text/x-rust':'rs', 'text/x-c':'c h i',
 'text/x-c++':'cpp cc cxx hpp hh hxx ino', 'text/x-shellscript':'sh bash zsh fish ksh csh command',
 'text/x-java':'java gradle groovy', 'text/x-kotlin':'kt kts', 'text/x-swift':'swift', 'text/x-go':'go sum',
 'text/x-ruby':'rb rake gemspec erb', 'text/x-php':'php phtml php3 php4 php5', 'text/x-perl':'pl pm pod',
 'text/x-lua':'lua', 'text/x-sql':'sql', 'text/x-r':'r rmd', 'text/x-dart':'dart', 'text/x-elixir':'ex exs',
 'text/x-erlang':'erl hrl', 'text/x-haskell':'hs lhs', 'text/x-clojure':'clj cljs cljc edn',
 'text/x-lisp':'lisp lsp cl el scm ss rkt', 'text/x-ocaml':'ml mli', 'text/x-fsharp':'fs fsx fsi',
 'text/x-fortran':'f f90 f95 for', 'text/x-pascal':'pas pp', 'text/x-asm':'asm s',
 'text/x-makefile':'make mk cmake ninja', 'text/x-diff':'diff patch', 'text/x-tex':'tex latex bib sty cls',
 'text/calendar':'ics ifb', 'text/vcard':'vcf vcard', 'application/pdf':'pdf',
 'image/png':'png apng', 'image/jpeg':'jpg jpeg jpe jfif', 'image/gif':'gif', 'image/webp':'webp',
 'image/avif':'avif', 'image/heic':'heic heif', 'image/tiff':'tif tiff', 'image/bmp':'bmp dib',
 'image/svg+xml':'svg', 'image/x-icon':'ico cur', 'image/x-portable-anymap':'pbm pgm ppm pnm pam',
 'image/x-raw':'raw cr2 cr3 nef nrw arw dng orf raf rw2 pef srw', 'image/vnd.adobe.photoshop':'psd psb',
 'image/x-xcf':'xcf', 'image/vnd.radiance':'hdr', 'image/ktx':'ktx ktx2', 'image/x-exr':'exr',
 'audio/mpeg':'mp3 mp2 mpa', 'audio/flac':'flac', 'audio/ogg':'ogg oga opus', 'audio/wav':'wav wave',
 'audio/aac':'aac m4a m4b', 'audio/aiff':'aif aiff', 'audio/midi':'mid midi kar', 'audio/x-ms-wma':'wma',
 'audio/x-module':'mod xm it s3m', 'audio/x-playlist':'m3u m3u8 pls cue',
 'video/mp4':'mp4 m4v f4v', 'video/webm':'webm', 'video/x-matroska':'mkv mk3d',
 'video/quicktime':'mov qt', 'video/x-msvideo':'avi', 'video/mpeg':'mpeg mpg mpe vob m2v ts2',
 'video/mp2t':'mts m2ts', 'video/ogg':'ogv', 'video/x-flv':'flv', 'video/x-ms-wmv':'wmv asf',
 'application/zip':'zip zipx jar war ear cbz', 'application/gzip':'gz tgz', 'application/x-xz':'xz txz',
 'application/x-bzip2':'bz2 tbz tbz2', 'application/zstd':'zst zstd', 'application/x-tar':'tar',
 'application/x-7z-compressed':'7z', 'application/vnd.rar':'rar cbr', 'application/x-lzip':'lz lzma',
 'application/x-compress':'z', 'application/x-archive':'a ar lib', 'application/x-cpio':'cpio',
 'application/vnd.debian.binary-package':'deb udeb', 'application/x-rpm':'rpm', 'application/x-iso9660-image':'iso img dmg',
 'application/vnd.openxmlformats-officedocument.wordprocessingml.document':'docx dotx',
 'application/msword':'doc dot', 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet':'xlsx xltx',
 'application/vnd.ms-excel':'xls xlt xlsm xlsb',
 'application/vnd.openxmlformats-officedocument.presentationml.presentation':'pptx potx ppsx',
 'application/vnd.ms-powerpoint':'ppt pot pps', 'application/vnd.oasis.opendocument.text':'odt ott',
 'application/vnd.oasis.opendocument.spreadsheet':'ods ots', 'application/vnd.oasis.opendocument.presentation':'odp otp',
 'application/rtf':'rtf', 'application/epub+zip':'epub', 'application/x-mobipocket-ebook':'mobi azw azw3',
 'font/ttf':'ttf ttc', 'font/otf':'otf', 'font/woff':'woff', 'font/woff2':'woff2',
 'application/x-executable':'exe com appimage bin run', 'application/x-sharedlib':'so dylib dll',
 'application/wasm':'wasm', 'application/x-object':'o obj', 'application/x-bytecode':'pyc pyo class',
 'application/vnd.android.package-archive':'apk aab', 'application/x-sqlite3':'sqlite sqlite3 db db3',
 'application/x-pem-file':'pem crt cer key pub p12 pfx', 'application/pgp-encrypted':'gpg pgp',
 'model/gltf+json':'gltf', 'model/gltf-binary':'glb', 'model/obj':'obj3d', 'model/stl':'stl',
 'model/3mf':'3mf', 'application/x-blender':'blend', 'application/x-fbx':'fbx', 'application/postscript':'ps eps ai',
 'application/x-qml':'qml qrc ui', 'application/x-vue':'vue svelte', 'text/x-zig':'zig',
}
rows=sorted((e,m) for m,es in groups.items() for e in es.split())
assert len(set(e for e,m in rows))==len(rows)
text='''// Tabla propia por extensión; fuente mantenida en scripts/generate-mime.py.
use std::path::Path;
pub const TABLE: &[(&str,&str)] = &[\n'''
text+=''.join(f'    ("{e}", "{m}"),\n' for e,m in rows)
text+=''' ];
pub fn mime(path:&Path)->&'static str {let ext=path.extension().unwrap_or_default().to_string_lossy().to_ascii_lowercase();TABLE.binary_search_by_key(&ext.as_str(),|&(e,_)|e).map(|i|TABLE[i].1).unwrap_or("application/octet-stream")}
#[cfg(test)]mod tests {use super::*;#[test]fn table_integrity(){assert!(TABLE.len()>=300);assert!(TABLE.windows(2).all(|w|w[0].0<w[1].0));assert_eq!(mime(Path::new("a.PNG")),"image/png");assert_eq!(mime(Path::new("a.unknown")),"application/octet-stream");}}
'''
Path('crates/core/src/mime.rs').write_text(text)
print(f'{len(rows)} extensiones')
