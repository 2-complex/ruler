<h2>Ruler</h2>

Ruler is a tool for managing a dependence graph of files.  It works with a <code>.rules</code> file.  A <code>.rules</code> file contains newline-separated blocks called <b>rules</b>.  Each <b>rule</b> consists of three sections: <b>targets</b>, <b>sources</b> and <b>command</b>.  Targets and sources are both lists of file paths.  Command is a command-line invocation that presumably takes the sources as input and updates the targets.  Each section ends with a single ":" alone on a line.  For example, a rule might look like this:

```rules
build/game
:
src/include/math.h
src/include/physics.h
src/game.cpp
:
c++
src/game.cpp
--std=c++17
-o
build/game
:
```

That would be a rule for building an executable using the commandline invocation `c++`.  The commandline invocation

```sh
ruler build
```

would check whether the target file `game` is up-to-date with its source files:

```
src/include/math.h
src/include/physics.h
src/game.cpp
```

If it is not, it runs the command:

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

That .rules file contains intermediate builds.  If that's the rules file, and we type:

```sh
ruler build
```

Ruler will execute the commands to build the intermeidate build targets: <code>build/math.o</code> and <code>build/physics.o</code> before finally building <code>build/game</code>.  What's more, Ruler will only execute the command to build a target that is out of date, so if <code>build/math.o</code> and <code>build/physics.o</code> have already been built, Ruler will not bother building them again.

Another ruler command is this:

```sh
ruler clean
```

That removes all files listed as targets in the <code>.rules</code> file.  Actually, that's only partly true.  It doesn't remove the files, it relocates them to a cache.  If a build executes and Ruler determines that some files can be recovered from the cache instead of being rebuilt, Ruler does that.

