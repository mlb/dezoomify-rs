# [dezoomify-rs](https://lovasoa.github.io/dezoomify-rs/)

[![Continuous Integration](https://github.com/lovasoa/dezoomify-rs/workflows/Continuous%20Integration/badge.svg)](https://github.com/lovasoa/dezoomify-rs/actions)

[**dezoomify-rs**](https://lovasoa.github.io/dezoomify-rs/) is a tiled image downloader.
Some webpages present high-resolution zoomable images without a way to download them.
These images are often *tiled*: the original large image has been split into smaller individual image files called tiles.
The only way to download such an image is to download all the tiles separately and then stitch them together.
This process can be automated by a tiled image downloader.

The most common tiled image downloader is probably [**dezoomify**](https://ophir.alwaysdata.net/dezoomify/dezoomify.html),
an online tool which is very easy to use.


The goal of this project is not to replace the traditional dezoomify.
However, some images are so large that they can't be efficiently downloaded and displayed inside a web browser.
Other times, a website tries to protect its tiles by refusing access to them when certain 
[HTTP headers](https://en.wikipedia.org/wiki/List_of_HTTP_header_fields) are not set to the right values.
**dezoomify-rs** is a desktop application for Windows, MacOs and linux that does not have the same limitations as the online zoomify.
dezoomify-rs also lets the user choose between
[several image formats](#supported-output-image-formats),
whereas in *dezoomify*, you can only save the image as *PNG*.

dezoomify-rs supports several zoomable image formats, each backed by a dedicated *dezoomer*.
The following dezoomers are currently available:
 - [**Google Arts & Culture**](#google-arts-culture) supports downloading images from
    [artsandculture.google.com](https://artsandculture.google.com/);
 - [**zoomify**](#zoomify) supports the popular zoomable image format *Zoomify*.
 - [**deepzoom**](#DeepZoom) supports Microsoft's *DZI* format (Deep Zoom Image),
 that is often used with the seadragon viewer.
 - [**IIIF**](#IIIF) supports the widely used International Image Interoperability Framework format.
 - [**Zoomify PFF**](#zoomify-pff) supports the old zoomify single-file image format.
 - [**Krpano**](#krpano) supports the [krpano](https://krpano.com/home/) panorama viewer
 - [**IIPImage**](#iipimage) supports the [iipimage](https://iipimage.sourceforge.io/) image format
 - [**NYPLImage**](#nyplimage) supports the [nypl](https://digitalcollections.nypl.org) image format
 - [**generic**](#Generic) For when the tile URLs follow a simple pattern.
 - [**custom**](#Custom-yaml) for advanced users.
   It allows you to specify a custom tile URL format that can contain multiple variables. This gives you the most flexibity, but requires some manual work.

## Screenshots

https://github.com/user-attachments/assets/e7174665-004d-44b2-a9f5-ae5a838e1262

## Usage instructions

### Download *dezoomify-rs*
First of all, you have to download the application.

 1. Go to the the [latest release page](https://github.com/lovasoa/dezoomify-rs/releases/latest),
 1. download the version that matches your operating system (Windows, MacOS, or Linux),
 1. Extract the binary from the compressed file.
 
On some operating systems, you may have to authorize the application execution
before being able to launch it. See how to do
[in MacOS](https://support.apple.com/kb/ph25088?locale=en_US).

### Install via Homebrew
As an alternative to installing the binary directly, on macOS and Linux dezoomify-rs is available via the [Homebrew package manager](https://brew.sh/). It can be installed with the command <code>brew install dezoomify-rs</code>.

## Supported output image formats

Dezoomify-rs supports multiple output image formats.
The format to use is determined by the name of the output file.
For instance, entering `dezoomify-rs http://example.com/ my_image.png` on the command line
will create a PNG image.

Each image format encoder has a distinct set of features and limitations :
 - **PNG** images are compressed losslessly, which means that the output image quality
   is (very slightly) better than JPEG, at the expense of much larger file sizes. 
   The PNG encoder in dezoomify-rs can create very large images;
   it is not limited by the available memory on your computer.
   This format is chosen by default when the image is very large,
   or its size is not known in advance. 
 - **JPEG** is the most common image format.
    JPEG images cannot be more than 65,535 pixels wide or high.
    This format is chosen be default for images that fit within this limit.
    The JPEG encoder in dezoomify-rs requires the whole image to fit in memory on your computer.
 - All formats [supported by image-rs](https://github.com/image-rs/image#21-supported-image-formats)
   are also supported.
 - [**IIIF**](https://iiif.io/), which allows you to re-create a zoomable image locally.
   This is the recommended output format when your image is very large
   (multiple hundreds of megapixels), since most image viewers do not accept huge PNGs or JPEGs.
   If the output path ends with `.iiif`, a folder will be created instead of a single file,
   with its structure following the IIIF specification.
   A file called `viewer.html` will be created inside this folder,
   which you can open in your browser to view the image.

## Tile cache

By default, dezoomify-rs works entirely in memory, which is very fast.
However, the latest versions added the possibility to use a "tile cache".
When you launch dezoomify-rs from the commandline with `dezoomify-rs --tile-cache my_caching_folder http://myurl.com`,
it will save all the image tiles it downloads to the specified folder.
If the download is interrupted before the end, you will be able to resume it later by specifying the same tile cache folder.
A tile cache also allows you to manually get the individual tiles if you want to stitch them manually.

## Dezoomers

### Google Arts Culture
In order to download images from google arts and culture, just open 
`dezoomify-rs`, and when asked, enter the URL of a viewing page, such as 
https://artsandculture.google.com/asset/light-in-the-dark/ZQFouDGMVmsI2w 

### Zoomify

You have to give dezoomify-rs an url to the `ImageProperties.xml` file.
You can use [dezoomify-extension](https://lovasoa.github.io/dezoomify-extension/) to
find the URL of this file.

Alternatively, you can find it out manually by opening your network inspector.
If the image tile URLs have the form
`http://example.com/path/to/TileGroup1/1-2-3.jpg`,
then the URL to enter is
`http://example.com/path/to/ImageProperties.xml`.

### IIIF

The IIIF dezoomer takes the URL of an
 [`info.json`](https://iiif.io/api/image/2.1/#image-information) file as input.
 
You can use [dezoomify-extension](https://lovasoa.github.io/dezoomify-extension/) to
find the URL of this file.

Alternatively, you can find this url in your browser's network inspector when loading the image.

### DeepZoom

The DeepZoom dezoomer takes the URL of a `dzi` file as input, which you can find using 
[dezoomify-extension](https://lovasoa.github.io/dezoomify-extension/).

You can find this url in your browser's network inspector when loading the image.
If the image tile URLs have the form
`http://test.com/y/xy_files/1/2_3.jpg`,
then the URL to enter is
`http://test.com/y/xy.dzi`.

### Zoomify PFF

[PFF](https://github.com/lovasoa/pff-extract/wiki/Zoomify-PFF-file-format-documentation)
is an old zoomable image file format format developed by zoomify.
You can give a pff meta-information URL (one that contains `requestType=1`)
to dezoomify-rs and it will download it. 

### Krpano

[Krpano](https://krpano.com/home/) is a zoomable image format often used
for panoramas, virtual tours, photoshperes, and other 3d zoomable images.
dezoomify-rs supports downloading individual image planes from such images.
You need to provide the xml meta-information file for the image.

### Nypl

The [digital collections of New York's Public Library](https://digitalcollections.nypl.org)
use their own zoomable image format, which dezoomify-rs supports.
Some images have a high-resolution version available, and work with this software.
Others do not, and can be downloaded by simply right-clicking on them in your browser.
To download an image, just enter the URL of its viewer page in dezoomify-rs, like for example:
 ```
 https://digitalcollections.nypl.org/items/a28d6e6b-b317-f008-e040-e00a1806635d
```

### IIPImage

[IIPImage](https://iipimage.sourceforge.io/) is an image web server that implements
the [Internet Imaging Protocol](https://iipimage.sourceforge.io/IIPv105.pdf).
Such images are easily recognizable by their tile URLs, which contain `FIF=`.
You can pass an URL containing `FIF=` to dezoomify-rs to let it download the image. 

### Generic

You can use this dezoomer if you know the format of tile URLs.
For instance, if you noticed that the URL of the first tile is 

```
http://example.com/my_image/image-0-0.jpg
```

and the second is 

```
http://example.com/my_image/image-1-0.jpg
```

then you can guess what the general format will be, and give dezoomify-rs
the following:

```
http://example.com/my_image/image-{{X}}-{{Y}}.jpg
```

If the numbers have leading zeroes in the URL
(such as `image-01-00.jpg` instead of `image-1-0.jpg`),
then you can specify them in the url template as follows:

```
http://example.com/my_image/image-{{X:02}}-{{Y:02}}.jpg
```

### Custom yaml

The [custom yaml dezoomer](https://github.com/lovasoa/dezoomify-rs/wiki/Usage-example-for-the-custom-YAML-dezoomer)
is a powerful tool that lets you download tiled images in many different formats, including formats that are not explicitly 
supported by dezoomify-rs.
In order to use this dezoomer, you'll need to create a `tiles.yaml` file, which is a little bit technical.
However, we have a [a tutorial for the custom YAML dezoomer](https://github.com/lovasoa/dezoomify-rs/wiki/Usage-example-for-the-custom-YAML-dezoomer)
to help you.
If you are having troubles understanding the tutorial or adapting it to your use-case, you should get in touch by
[opening a new github issue](https://github.com/lovasoa/dezoomify-rs/issues?q=).

## Command-line options

When using dezoomify-rs from the command-line

```
Allows downloading zoomable images. Supports several different formats such as zoomify, iiif, and deep zoom images.

Usage: dezoomify-rs [OPTIONS] [INPUT_URI] [OUTFILE]

Arguments:
  [INPUT_URI]  Input URL or local file name. By default, the program will ask for it interactively
  [OUTFILE]    File to which the resulting image should be saved. By default the program will generate a name based on the image metadata if available. Otherwise, it will generate a name in the format "dezoomified[_N].{jpg,png}" depending on which files already exist in the current directory, and whether the target image size fits in a JPEG or not

Options:
  -?, --help
          Displays this help message
  -d, --dezoomer <DEZOOMER>
          Name of the dezoomer to use [default: auto]
  -l, --largest
          If several zoom levels are available, then select the largest one
  -w, --max-width <MAX_WIDTH>
          If several zoom levels are available, then select the one with the largest width that is inferior to max-width
  -h, --max-height <MAX_HEIGHT>
          If several zoom levels are available, then select the one with the largest height that is inferior to max-height
      --zoom-level <ZOOM_LEVEL>
          Select a specific zoom level by its index (0-based). If the specified level doesn't exist, falls back to the last one
  -n, --parallelism <PARALLELISM>
          Degree of parallelism to use. At most this number of tiles will be downloaded at the same time [default: 16]
  -r, --retries <RETRIES>
          Number of new attempts to make when a tile load fails before giving up. Setting this to 0 is useful to speed up the generic dezoomer, which relies on failed tile loads to detect the dimensions of the image. On the contrary, if a server is not reliable, set this value to a higher number [default: 1]
      --retry-delay <RETRY_DELAY>
          Amount of time to wait before retrying a request that failed. Applies only to the first retry. Subsequent retries follow an exponential backoff strategy: each one is twice as long as the previous one [default: 2s]
      --compression <COMPRESSION>
          A number between 0 and 100 expressing how much to compress the output image. For lossy output formats such as jpeg, this affects the quality of the resulting image. 0 means less compression, 100 means more compression. Currently affects only the JPEG and PNG encoders [default: 20]
  -H, --header <HEADERS>
          Sets an HTTP header to use on requests. This option can be repeated in order to set multiple headers. You can use `-H "Referer: URL"` where URL is the URL of the website's viewer page in order to let the site think you come from the legitimate viewer
      --max-idle-per-host <MAX_IDLE_PER_HOST>
          Maximum number of idle connections per host allowed at the same time [default: 32]
      --accept-invalid-certs
          Whether to accept connecting to insecure HTTPS servers
  -i, --min-interval <MIN_INTERVAL>
          Minimum amount of time to wait between two consequent requests. This throttles the flow of image tile requests coming from your computer, reducing the risk of crashing the remote server of getting banned for making too many requests in a short succession [default: 50ms]
      --timeout <TIMEOUT>
          Maximum time between the beginning of a request and the end of a response before the request should be interrupted and considered failed [default: 30s]
      --connect-timeout <CONNECT_TIMEOUT>
          Time after which we should give up when trying to connect to a server [default: 6s]
      --logging <LOGGING>
          Level of logging verbosity. Set it to "debug" to get all logging messages [default: warn]
  -c, --tile-cache <TILE_STORAGE_FOLDER>
          A place to store the image tiles when after they are downloaded and decrypted. By default, tiles are not stored to disk (which is faster), but using a tile cache allows retrying partially failed downloads, or stitching the tiles with an external program
      --bulk <BULK>
          Path to a text file containing a list of URLs to process in bulk mode. Each line in the file should contain one URL. In bulk mode, if no level-specifying argument is defined (such as --max-width), then --largest is implied
  -V, --version
          Print version
```

## Documentation
  - For documentation specific to this tool, see the [dezoomify-rs wiki](https://github.com/lovasoa/dezoomify-rs/wiki). Do not hesitate to contribute to it by creating new pages or modifying existing ones.
  - For general purpose documentation about zoomable images, the [dezoomify wiki](https://github.com/lovasoa/dezoomify/wiki) may be useful.

## Bulk mode

dezoomify-rs supports bulk processing of multiple URLs using the `--bulk` option. This allows you to process multiple zoomable images in a single command.

### Using bulk mode

Create a text file containing one URL per line:

```
# urls.txt - Lines starting with # are comments and will be ignored
https://example.com/image1/ImageProperties.xml
https://example.com/image2/info.json
https://example.com/image3.dzi

# You can also include local file paths
/path/to/local/tiles.yaml
```

Then run dezoomify-rs with the `--bulk` option:

```sh
./dezoomify-rs --bulk urls.txt
```

### Bulk mode behavior

- **Progress tracking**: Each URL is processed sequentially with progress indicators (`[1/5]`, `[2/5]`, etc.)
- **Automatic level selection**: If no level-specifying arguments (`--max-width`, `--max-height`, `--zoom-level`) are provided, `--largest` is automatically implied
- **Output file naming**: If you specify an output file with `--outfile`, each image will be saved with a suffix (`_0001`, `_0002`, etc.)
- **Error handling**: Failed downloads don't stop the entire process; the tool continues with the next URL and reports a summary at the end

### Examples

Process multiple images and save them with a common prefix:
```sh
./dezoomify-rs --bulk urls.txt my_collection.jpg
# Creates: my_collection_0001.jpg, my_collection_0002.jpg, etc.
```

Process with specific size constraints:
```sh
./dezoomify-rs --bulk urls.txt --max-width 2000
# Uses max-width constraint instead of auto-selecting largest
```

### Alternative: Using shell scripts

For more advanced processing, you can still use traditional shell scripting approaches with [xargs](https://en.wikipedia.org/wiki/Xargs):

```sh
xargs -d '\n' -n 1 ./dezoomify-rs < ./urls.txt
```

## Technical Architecture

Dezoomify-rs works by first analyzing the structure of a zoomable image to understand its tiling scheme,
then systematically downloads all individual tiles and reassembles them into the complete high-resolution image.

We can automatically detect which type of zoomable image format is being used (such as Zoomify, IIIF, or DeepZoom)
and apply the appropriate strategy to locate and download tiles.

During the download process, dezoomify-rs can preserve important image metadata:
[ICC color profiles](https://en.wikipedia.org/wiki/ICC_profile) from individual tiles are transferred
to the final output image to maintain accurate color representation,
and EXIF metadata is preserved when saving to PNG format (though it's lost with other formats due to encoder limitations).

The PNG encoder uses a streaming approach that writes image data progressively to disk, allowing it to handle extremely large images without being limited by available system memory. In contrast, other format encoders like JPEG must keep the entire assembled image in memory before writing, which can be a constraint for very large images.
