# P2P File Transfer

## Overview

This is a small project to help solve a problem that should've been solved by now: file transfer.

For in-person file transfers, we have lots of solutions: Google's Nearby Share and Apple's Airdrop are good examples. But for long-distance ones, the best we've been able to do is just upload the content to a third-party server for everyone to download from. This is simply wasteful and costly.

Why not just put the huge amount of resources residential Internet users get to use and DIY it?

## Tech

It is being coded in Rust in order to get both safety and performance, using the Tokio asynchronous framework in order to get faster performance.

Both the client and server have async network request support.

## Build Instructions

You can use Nix and direnv to setup the same environment as me for compiling the program from source. Simply clone this repo, then use `nix develop` or allow direnv to
get all the dev tools setup. Then compile the program using `cargo build`.

You can also just use `rustup` to get the Rust development toolchain and then call `cargo build`. We use `rustls`, so we don't really need any system libraries for TLS etc, just Rust is enough.

## Usage

If you want to send a file, run `file_transfer server <filename> <secret>`, where `<filename>` is the file you wish to transfer and `<secret>` is a small string
that the client will send in order to prove it is authorized to get that file (you will need to send this secret to any parties you wish to send files to).
The command will also end up generating your config (if it doesn't exist) which includes your certificate.
This self-signed certificate is generated once and serves as your identity. For a client to trust that it is indeed you and not some attacker impersonating you,
you need to tell the client (or rather, whoever you wish to send the file to) to add your certificate to the keystore.

For this, you can copy paste the public key that the invocation of `file_transfer server` will print, and send it to your recipient over a trusted medium
(encrypted messaging apps like Signal or Matrix). The other party will in turn run `file_transfer add-key <key>`, where `<key>` is
simply the public key that you send.

Once the key exchange is done, the other party can run `file_transfer client <filename> <secret>` on their machine. Note that the filename here needs to be exactly same
as on the server invocation. Simply exchange external IP:port values and let the transfer happen on its own.

This may seem complicated, but most of this is a one-time setup; for future usage you need only run the client and server, exchange IP:port values and secret and hit Enter.
All this work is to ensure that your files arrive over the Internet, encrypted and authenticated.

## Implemented Features

- Custom QUIC-based basic protocol to transfer files as well as provide encryption.

- NAT traversal code which temporarily opens up ports between client and server to communicate with each other.

- A basic level of support for authentication and authorization. Right now, the client checks trust that the server is not legitimate by checking its keystore.

In turn, the server checks that the client knows a particular secret that is sent over the wire (encrypted) once the client is sure that the server is legitimate.
 
## Goals

- Send client and server data along with public keys in a user-friendly way: just have both parties send a small code which encodes all this info so they don't have to type it out. The exact details can be worked out later.

- Send multiple files at once. This will require some additional handshaking implemented in the custom protocol

- Server should be able to perform NAT traversal for a new client that wants to connect to a running instance instead of having to run another instance of the program

## Status

Alpha: I am basically learning network programming through this. This is in NO way usable right now.

Right now, it is able to make direct connections through easy NAT and transfer one file from one machine (called a server) to another machine (called a client) over the Internet.