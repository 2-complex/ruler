<h2>Ruler</h2>

Ruler is a tool for managing a dependence graph of files.  It applies in any situation where a commandline executable takes files as input (sources) and generates files as output (targets).  A large-scale C/C++ project is a good example of such a situation.  C/C++ projects have lots of intermediate build targets.  With the right dependence graph set up, Ruler can help make intermediate builds faster by building only what's necessary.  C/C++ is not the only use-case, however.  Many problems can be solved by dependence management.

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
-o build/game
:
```

That rule declares that the executable `build/game` depends on three source files, and builds by this line:

```sh
c++ game.cpp --std=c++17 -o build/game
```

(Note: Ruler uses slightly unconventional syntax for the commandline so that one invocation can span multiple lines without the need for backslashes.  To get a multi-line invocation, separate by two newlines.)

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

With that `.rules` file, if we type...

```sh
ruler build
```

... Ruler will execute the commands to build the intermeidate targets: <code>build/math.o</code> and <code>build/physics.o</code> before finally building <code>build/game</code>.  What's more, Ruler will only execute the command to build a target that is out-of-date, so if <code>build/math.o</code> and <code>build/physics.o</code> have already been built, Ruler will not bother building them again.

Another ruler command is this:

```sh
ruler clean
```

That removes all files listed as targets in the <code>.rules</code> file.  Actually, that is only partly true.  Rather than remove the files, it relocates them to a cache.  If a build is invoked and Ruler determines that some files already reside in the cache, Ruler recovers them, rather than rebuilding.

The cache also gets populated when intermediate build results are replaced.  So, if you edit a source file, type `ruler build`, then undo the edit and `ruler build` again, Ruler appeals to the cache and recovers the target instead of rebuilding it.

