#!/bin/bash

prog=target/thumbv7em-none-eabihf/release/matrixled
openocd -f flash.cfg -c "flash_elf $prog"
