# Discord As A Filesystem V2

Simple nbd server that allows you to mount discord as a filesystem.

_Sorry for messy code, I made it to work, not to be pretty._

## Why? (again)

I had a few ideas how to make this faster and more reliable, so I decided to make a new version of this.
Also because the old version was a mess.

## Is it faster?

Yes! This version implements cache and a sync queue, so it's much faster than the old version.

## How do I use it?

First, you need to clone this repo using git:

```bash
git clone https://github.com/olix3001/DAAFS
```

Then, rename `.env.example` to just `.env` and fill it with your bot token and the channel id you want to mount.

_Note_: Everything in .env is compiled into the binary, so you should be careful with it. (and remember to recompile the binary if you change it)

Then, you need to compile the binary and run it. Happily, this can be done with just one command:

```bash
./run.sh
```

## How does It work?

It uses two types of messages: `METABLOCK` and `DATA PAGE`. First one is used to hold pointers to data pages and zero-masks (more about them later), and the second one is used to hold actual data.

## How to connect to it?

You can use `nbd-client` to connect to it. Example:

```bash
nbd-client localhost /dev/nbd0
```

Then if you want to mount it, you can format it with your favorite filesystem and mount it.

_example for fat16_

```bash
mkfs.vfat -F16 /dev/nbd0
mkdir /mnt/discord
mount /dev/nbd0 /mnt/discord
```

## How to disconnect from it?

First unmount it, then disconnect from it.

```bash
umount /mnt/discord
nbd-client -d /dev/nbd0
```

If run.sh doesn't want to stop, disconnecting from it should fix it.

## What about WSL?

For this to work on wsl you need to have custom kernel with nbd support.

## What about Windows?

I am working on it, but you probably need to have wnbd installed.

# **IMPORTANT**

This is for educational purposes only. I **DO NOT** recommend storing any important data on this. I am not responsible for any data loss. Use at your own risk.

Also I am not responsible for any bans that you may get for using this. I am not sure if this is against discord TOS or not. Use at your own risk.

You can use It but it is better not to store more than like 64/128MB of data on it as discord may get mad at you.
