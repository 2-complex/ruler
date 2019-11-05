def check_python_3():
    import sys
    if sys.version_info < (3, 0):
        raise "Must use python 3.0 or greater"
    else:
        pass

check_python_3()

import unittest
import shutil
import os
import subprocess
import random


def enter_directory(name):
    try:
        os.mkdir(name)
    except:
        pass
    os.chdir(name)


def make_rules(deps):
    dep_map = {}

    for s,t in deps:
        dep_map[t] = dep_map.get(t, [])
        dep_map[t].append(s)

    with open("build.rules", "w") as f:
        rules = []
        for target in dep_map:
            rule = []
            rule.append(target)
            rule.append(":")
            for source in dep_map[target]:
                rule.append(source)
            rule.append(":")
            rule.append("../../poemcat")
            for source in dep_map[target]:
                rule.append(source)
            rule.append("--target="+target)
            rule.append(":")
            rules.append("\n".join(rule))

        content = "\n\n".join(rules) + "\n"
        f.write(content)


def touch_leaves(deps):
    firsts = set()
    seconds = set()

    for s,t in deps:
        firsts.add(s)
        seconds.add(t)

    for name in firsts - seconds:
        with open(name, "w") as f:
            f.write(name)


class TestsInDirectories(unittest.TestCase):
    def setUp(self):
        dir_name = str(self.id()) + "-directory"
        os.mkdir(dir_name)
        enter_directory(dir_name)

    def tearDown(self):
        dir_name = str(self.id()) + "-directory"
        os.chdir("..")
        shutil.rmtree(dir_name)

    def do_command_expect(self, line, expected_out, expected_err):
        result = subprocess.run(line, shell=True, check=True, capture_output=True)
        self.assertEqual(result.stdout, expected_out)
        self.assertEqual(result.stderr, expected_err)

    def basic(self, deps, target):
        make_rules(deps)
        touch_leaves(deps)

        all_files = set()
        for s,t in deps:
            all_files.add(s)
            all_files.add(t)

        expected_out = bytes("" + "\n".join(sorted(list(all_files))) + "\n", "utf8")

        self.do_command_expect("../../target/debug/ruler build " + target, expected_out, b"")

    def test_AB(self):
        self.basic([("A", "B")], "B")

    def test_ABC(self):
        self.basic([("A", "C"), ("B", "C")], "C")

    def test_ABCDE(self):
        self.basic([("A", "C"), ("B", "C"), ("A", "D"), ("B", "D"), ("C", "E"), ("D", "E")], "E")


if __name__ == '__main__':
    if not os.path.isdir("test-sandbox"):
        os.mkdir("test-sandbox")

    os.chdir("test-sandbox")
    unittest.main()
    os.chdir("..")
    shutil.rmtree("test-sandbox")
