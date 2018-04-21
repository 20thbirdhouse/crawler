# crawler
`crawler` is a web-crawler for the 20<sup>th</sup> Birdhouse project. It scans
the Web (or theoretically every protocol supported by 
[reqwest](https://github.com/seanmonstar/reqwest)) for URLs, and parses HTML
with Servo's [html5ever](https://github.com/servo/html5ever).

## Usage
Run with cargo. It's recommended to provide `RUST_LOG=crawler=info` to get its
status as it crawls. Provide a URL to start with as well.
```sh
RUST_LOG=crawler=info cargo run https://github.com
```
