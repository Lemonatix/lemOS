# lemOS/Makefile
#
# Usage examples (run from lemOS/):
#   make build TARGET=userland/hello
#   make run   TARGET=userland/hello
#   make clean TARGET=userland/hello
#
# If TARGET is omitted, it defaults to userland/hello.

CC      = gcc
CFLAGS  = -Wall -Wextra -std=c11 -O2

# Default target if none is specified
TARGET ?= userland/hello

SRC  := $(TARGET).c
OBJ  := $(TARGET).o
BIN  := $(TARGET)

.PHONY: all build run clean

all: build

# Build just compiles the selected TARGET
build: $(BIN)

$(BIN): $(OBJ)
	$(CC) $(CFLAGS) -o $@ $^

$(OBJ): $(SRC)
	$(CC) $(CFLAGS) -c $< -o $@

# Run from within the directory of the binary
run: $(BIN)
	cd $(dir $(BIN)) && ./$(notdir $(BIN))

clean:
	rm -f $(BIN) $(OBJ)