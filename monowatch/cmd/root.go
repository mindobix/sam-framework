package cmd

import (
	"fmt"
	"os"

	"github.com/fatih/color"
)

// rootCmd represents the base command when called with no subcommands.
// We keep this minimal; the two real commands are check and install.

// Execute is called from main.go.
func Execute() error {
	root := newRoot()
	root.AddCommand(newCheckCmd())
	root.AddCommand(newInstallCmd())
	return root.Execute()
}

// newRoot creates the cobra root command.
func newRoot() *rootCommand {
	// We use a hand-rolled minimal root to avoid pulling cobra into go.mod
	// (the CLAUDE.md spec says "no external dependencies beyond stdlib" except
	// fatih/color which is already listed in go.mod).
	return &rootCommand{}
}

// rootCommand is a minimal command dispatcher.
type rootCommand struct {
	subcommands []*subcommand
}

type subcommand struct {
	name string
	run  func(args []string) error
}

func (r *rootCommand) AddCommand(s *subcommand) {
	r.subcommands = append(r.subcommands, s)
}

func (r *rootCommand) Execute() error {
	args := os.Args[1:]
	if len(args) == 0 {
		r.usage()
		return nil
	}

	name := args[0]
	for _, sub := range r.subcommands {
		if sub.name == name {
			return sub.run(args[1:])
		}
	}

	if name == "--help" || name == "-h" || name == "help" {
		r.usage()
		return nil
	}

	fmt.Fprintf(os.Stderr, "%s: unknown command %q\n", color.New(color.Bold).Sprint("monowatch"), name)
	r.usage()
	os.Exit(1)
	return nil
}

func (r *rootCommand) usage() {
	fmt.Fprintln(os.Stderr, "Usage: monowatch <command> [flags]")
	fmt.Fprintln(os.Stderr)
	fmt.Fprintln(os.Stderr, "Commands:")
	for _, sub := range r.subcommands {
		fmt.Fprintf(os.Stderr, "  %s\n", sub.name)
	}
	fmt.Fprintln(os.Stderr)
	fmt.Fprintln(os.Stderr, "Run 'monowatch <command> --help' for more information.")
}
