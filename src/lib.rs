//! I want to use Rust to write high-speed, un-pwn-able parsers.  This
//! seems like a killer application for libraries written in Rust.  Below,
//! I compare the performance and API of copying parsers versus zero-copy
//! parsers.  Anyway, here's the key benchmark:
//!
//! ```
//! test copying_parser   ... bench:     90432 ns/iter (+/- 5644) = 26 MB/s
//! test zero_copy_parser ... bench:      1926 ns/iter (+/- 135) = 1246 MB/s
//! ```
//!
//! My goal: Can we make zero_copy_parser an instance of Iterator?  Or does
//! Iterator force us to use something like copying_parser?
//!
//! And if Iterator does force us to copy, is there some way to change
//! Iterator that allows us to use zero_copy_parser without causing ugly
//! design issues elsewhere?

extern crate test;
use std::iter::range;


//=========================================================================
// Infrastructure

static LINE: &'static str = "foo bar baz\n";

/// This is our raw data source.  Pretend it's on disk somewhere, and it's
/// too big to load into memory all at once.
pub fn make_pretend_file() -> String {
    let mut result: String = String::new();
    for _ in range(0u, 200) { result.push_str(LINE); }
    result
}

/// This is our stand-in for a smart implementation of the Buffer trait.
/// In the real world, it has an internal buffer of some sort, and it has
/// some magic to finesse buffer boundaries for us (in an amortitized
/// fashion), so we always get all the data associated with a given
/// iteration.
pub struct BufferedReader<'a> {
    file: &'a str,
    offset: uint
}

impl<'a> BufferedReader<'a> {
    /// Create a new BufferedReader.
    pub fn new<'a>(file: &'a str) -> BufferedReader<'a> {
        BufferedReader{file: file, offset: 0}
    }

    /// Return a line with no allocations.  Again, a massive
    /// oversimplification: We're assuming our return value points into an
    /// I/O buffer.  The analogous read-world function is Buffer::fill_buf,
    /// plus some custom magic to get us complete lines.
    #[inline]
    pub fn next_line<'a>(&'a mut self) -> Option<&'a str> {
        if self.offset == self.file.len() { return None; }
        let result = self.file.slice(self.offset, self.offset + LINE.len());
        self.offset += LINE.len();
        Some(result)
    }
}


//=========================================================================
// CopyingParser

pub struct CopyingParser<'a> {
    reader: &'a mut BufferedReader<'a>
}

impl<'a> CopyingParser<'a> {
    pub fn new(reader: &'a mut BufferedReader<'a>) -> CopyingParser<'a> {
        CopyingParser{reader: reader}
    }
}

impl<'a> Iterator<(String,String,String)> for CopyingParser<'a> {
    // We can use the iterator protocol here, but we need to copy.
    fn next(&mut self) -> Option<(String,String,String)> {
        match self.reader.next_line() {
            None => None,
            Some(line) => {
                Some((line.slice(0, 3).to_string(),
                      line.slice(4, 7).to_string(),
                      line.slice(8, 11).to_string()))
            }
        }
    }
}

#[bench]
fn copying_parser(b: &mut test::Bencher) {
    let file = make_pretend_file();
    b.bytes = file.len() as u64;
    b.iter(|| {
        let mut reader = BufferedReader::new(file.as_slice());
        let mut parser = CopyingParser::new(&mut reader);
        // This looks nice, but it's really slow.
        for result in parser {
            test::black_box(result);
        }
    });
}


//=========================================================================
// ZeroCopyParser

pub struct ZeroCopyParser<'a> {
    reader: &'a mut BufferedReader<'a>,
}

impl<'a> ZeroCopyParser<'a> {
    pub fn new(reader: &'a mut BufferedReader<'a>) -> ZeroCopyParser<'a> {
        ZeroCopyParser{reader: reader}
    }

    // I can't figure out how to use the iterator protocol here.  The key
    // insight is that this signature works like
    // BufferedReader::next_line() or Buffer::fill_buf().  Our return value
    // locks the iterator as immutable until it goes out of scope.  But see
    // https://github.com/rust-lang/rust/issues/6393 and
    // https://github.com/rust-lang/rust/issues/12147 for some lifetime
    // checker iteractions that can be tricky when working with conditional
    // returns.
    pub fn next(&mut self) -> Option<(&str, &str, &str)> {
        match self.reader.next_line() {
            None => None,
            Some(ref line) => {
                // Like above, but keep our strings in BufferedReader's
                // internal buffer.
                Some((line.slice(0, 3),
                      line.slice(4, 7),
                      line.slice(8, 11)))
                
            }
        }
    }
}

// I tried:
//
// ```
// impl<'a> Iterator<(&'a str,&'a str,&'a str)> for ZeroCopyParser<'a> {
//    fn next(&mut self) -> Option<(&str, &str, &str)> {
// ```
//
// But I got:
//
// method `next` has an incompatible type for trait: expected concrete
// lifetime, found bound lifetime parameter

#[bench]
fn zero_copy_parser(b: &mut test::Bencher) {
    let file = make_pretend_file();
    b.bytes = file.len() as u64;
    b.iter(|| -> () {
        let mut reader = BufferedReader::new(file.as_slice());
        let mut parser = ZeroCopyParser::new(&mut reader);
        // This looks ugly, but it's really, really fast.  We're using a
        // mutable borrow of parser.next() to lock our internal buffers
        // into place until the borrow expires.
        loop {
            match parser.next() {
                None => { break; }
                Some(line) => { test::black_box(line); }
            }
        }
    });
}
