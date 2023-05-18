# Multifis

Have you ever wanted to upload a 70TB file to your instance's Effis but couldn't because
your instance admin decided to limit the individual file size to 500KB? No worries!

With Multifis you can upload your 70TB file as 1.4 million small files and one meta-file
which has all of the IDs of the other files, sounds too good to be true, amirite?

Disclaimer: Do not use this as a way to spam instances with a lot of files, instance
admins *can* and *will* ban you. I am not responsible for individual behaviour.

## Usage

```sh
$ multifis upload <instance url> <file>
123312093 # some random file ID
$ multifis download <instance url> <file id>
Finished installing <file>.
```

## Installing

```sh
cargo install --git https://github.com/enokiun/multifis
```
