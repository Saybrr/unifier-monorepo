


Registry find downloader should be based on the json parsing, not url type
- Instead of the registry finding which downloader, the download method should be attached to the struct

- we don't need to build a registry ourselves, that should be done by the parser
- once parsing the list, we know what downloaders we will need to use
- the config can be created by us, each downloader should be able to handle that config






