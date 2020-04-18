#!/bin/bash

iconv -t EUCJP | od -An -tx1 --endian=big | awk  '{c="0x"$1;for(i=2;i<NF;i++) c=c",0x"$i; print c}'
