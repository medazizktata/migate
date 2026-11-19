package main

import (
	"fmt"
	"time"
)

// calibration: identical to hello-go plus a 5 ms sleep
func main() { time.Sleep(5 * time.Millisecond); fmt.Println("hello") }
