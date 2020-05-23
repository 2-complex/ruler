<h2>Ruler</h2>

Ruler is a tool for managing a dependence graph of files.  It applies in any scenario where a commandline executable takes files as input (sources) and generates files as output (targets).  A large-scale C/C++ based project with lots of intermediate build results presents such a situation, however, Ruler is detatched from the idea that this its only use-case.  Many problems can be solved by dependence management.

Dependencies are encoded in a <code>.rules</code> file.  A <code>.rules</code> file contains newline-separated blocks called <b>rules</b>.  Each <b>rule</b> consists of three sections: <b>targets</b>, <b>sources</b> and <b>command</b>.  Targets and sources are newline-separated lists of file paths.  Command is a command-line invocation that presumably takes the sources as input and updates the targets.  Each section ends with a single ":" alone on a line.  For example, a `.rules` might contain this single rule:

```rules
build/game
:
src/game.h
src/game.cpp
:
c++
src/game.cpp
--std=c++17
-o
build/game
:
```

This declares that the executable `build/game` depends on three source files, and builds by this `c++`command-line invocation:

```sh
c++ game.cpp --std=c++17 -o build/game
```

With the above`.rules` file, if we type this:

```sh
ruler build
```

Ruler checks whether the target file `build/game` is up-to-date with its source files:

```
src/game.h
src/game.cpp
```

If it is not, Ruler runs the command:

```sh
c++ game.cpp --std=c++17 -o build/game
```

In general, a `.rules` file can contain lots of rules, each separated from the next by a single empty line, like this:

```rules
build/game
:
src/include/math.h
src/include/physics.h
build/math.o
build/physics.o
src/game.cpp
:
c++
build/math.o
build/physics.o
src/game.cpp
--std=c++17
-o
build/game
:

build/math.o
:
src/include/math.h
src/math.cpp
:
c++
--std=c++17
-c
src/math.cpp
-o
build/math.o
:

build/physics.o
:
src/include/math.h
src/physics.cpp
:
c++
--std=c++17
-c
src/physics.cpp
-o
build/physics.o
:
```

That `.rules` file contains intermediate targets.  with that `.rules` file, and we type:

```sh
ruler build
```

Ruler will execute the commands to build the intermeidate targets: <code>build/math.o</code> and <code>build/physics.o</code> before finally building <code>build/game</code>.  What's more, Ruler will only execute the command to build a target that is out-of-date, so if <code>build/math.o</code> and <code>build/physics.o</code> have already been built, Ruler will not bother building them again.

Another ruler command is this:

```sh
ruler clean
```

That removes all files listed as targets in the <code>.rules</code> file.  Actually, that is only partly true.  Rather than remove the files, it relocates them to a cache.  If a build is invoked and Ruler determines that some files already reside in the cache, Ruler recovers them, rather than rebuilding.

