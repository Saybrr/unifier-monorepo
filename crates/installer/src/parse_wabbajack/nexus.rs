/*

we have a list of files to download in the wabbjack maifest

each file has its own declared downloader. I have written the underlying downloading logic for an httpdownloader

this should be expanded to accomodate different "downloader" types:

in addition to actually downloading the file, the surrounding logic will be different

for example: the GameFile type will copy a game file from on disk

we want all these to be included in the multithreaded process currentlu implemented by downloader.

*/