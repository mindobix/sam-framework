package internal

import (
	"fmt"
	"io"
	"sort"
	"strings"

	"github.com/fatih/color"
)

// ─── colour helpers ────────────────────────────────────────────────────────────

var (
	boldRed    = color.New(color.FgRed, color.Bold)
	boldYellow = color.New(color.FgYellow, color.Bold)
	boldGreen  = color.New(color.FgGreen, color.Bold)
	dimWhite   = color.New(color.Faint)
	bold       = color.New(color.Bold)
)

func riskColor(risk string) *color.Color {
	switch strings.ToLower(risk) {
	case "critical":
		return boldRed
	case "high":
		return boldYellow
	case "medium":
		return color.New(color.FgYellow)
	default:
		return color.New(color.Reset)
	}
}

func edgeLabel(edgeType string, score float64) string {
	switch edgeType {
	case "co_change":
		return dimWhite.Sprintf("co-change (%.2f)", score)
	default:
		return "static import"
	}
}

func callsFormat(n int64) string {
	switch {
	case n >= 1_000_000:
		return fmt.Sprintf("%.1fM", float64(n)/1_000_000)
	case n >= 1_000:
		return fmt.Sprintf("%.1fK", float64(n)/1_000)
	default:
		return fmt.Sprintf("%d", n)
	}
}

// ─── column widths ─────────────────────────────────────────────────────────────

func maxLen(strs []string) int {
	m := 0
	for _, s := range strs {
		if len(s) > m {
			m = len(s)
		}
	}
	return m
}

// ─── ImpactTable ──────────────────────────────────────────────────────────────

// ImpactTable renders the full MonoWatch output to w.
//
//	SAM MonoWatch — impact analysis
//	━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//	Changed: shared/auth/token.go
//	         shared/auth/session.go
//
//	Domain              Risk        Calls/day   Type
//	──────────────────────────────────────────────────
//	apis/payments       CRITICAL    2.1M        static import
//	...
//	──────────────────────────────────────────────────
//	2 critical · 1 high · 0 medium · 3 low · 5 not affected
//	Push proceeding (block_on_critical = false)
func ImpactTable(w io.Writer, result *ImpactResponse, changedFiles []string,
	blockOnCritical bool, blocked bool) {

	const divWide = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	const divNarrow = "──────────────────────────────────────────────────────────────────"

	fmt.Fprintln(w, bold.Sprint("SAM MonoWatch")+" — impact analysis")
	fmt.Fprintln(w, divWide)

	// Changed files section.
	if len(changedFiles) > 0 {
		fmt.Fprintln(w, "Changed:", changedFiles[0])
		for _, f := range changedFiles[1:] {
			fmt.Fprintln(w, "        ", f)
		}
		fmt.Fprintln(w)
	}

	if len(result.Entries) == 0 {
		boldGreen.Fprintln(w, "No cross-domain impact detected.")
		fmt.Fprintln(w, divWide)
		return
	}

	// Sort entries: critical → high → medium → low, then alphabetical within tier.
	sorted := make([]ImpactEntry, len(result.Entries))
	copy(sorted, result.Entries)
	riskOrder := map[string]int{"critical": 0, "high": 1, "medium": 2, "low": 3}
	sort.SliceStable(sorted, func(i, j int) bool {
		oi := riskOrder[strings.ToLower(sorted[i].Risk)]
		oj := riskOrder[strings.ToLower(sorted[j].Risk)]
		if oi != oj {
			return oi < oj
		}
		return sorted[i].Domain < sorted[j].Domain
	})

	// Compute column widths.
	domains := make([]string, len(sorted))
	for i, e := range sorted {
		domains[i] = e.Domain
	}
	domainW := maxLen(domains)
	if domainW < len("Domain") {
		domainW = len("Domain")
	}

	// Header.
	header := fmt.Sprintf("%-*s  %-8s  %-9s  %s",
		domainW, "Domain", "Risk", "Calls/day", "Type")
	fmt.Fprintln(w, bold.Sprint(header))
	fmt.Fprintln(w, divNarrow)

	// Rows.
	counts := map[string]int{"critical": 0, "high": 0, "medium": 0, "low": 0}
	for _, e := range sorted {
		risk := strings.ToUpper(e.Risk)
		col := riskColor(e.Risk)

		row := fmt.Sprintf("%-*s  %-8s  %-9s  %s",
			domainW, e.Domain,
			col.Sprint(risk),
			callsFormat(e.CallsDay),
			edgeLabel(e.EdgeType, e.Score),
		)
		fmt.Fprintln(w, row)

		key := strings.ToLower(e.Risk)
		if _, ok := counts[key]; ok {
			counts[key]++
		}
	}

	fmt.Fprintln(w, divNarrow)

	// Summary line.
	summary := fmt.Sprintf("%s critical · %s high · %d medium · %d low",
		boldRed.Sprint(counts["critical"]),
		boldYellow.Sprint(counts["high"]),
		counts["medium"],
		counts["low"],
	)
	fmt.Fprintln(w, summary)
	fmt.Fprintln(w)

	// Decision line.
	if blocked {
		boldRed.Fprintln(w, "Push BLOCKED (block_on_critical = true in .sam/config.yaml)")
		fmt.Fprintln(w, "To override: git push --no-verify")
	} else if blockOnCritical && counts["critical"] > 0 {
		boldRed.Fprintln(w, "Push BLOCKED (block_on_critical = true in .sam/config.yaml)")
		fmt.Fprintln(w, "To override: git push --no-verify")
	} else {
		dimWhite.Fprintln(w, "Push proceeding (block_on_critical = false)")
	}

	fmt.Fprintln(w, divWide)
}

// NoImpact prints a compact "no cross-domain impact" message.
func NoImpact(w io.Writer) {
	boldGreen.Fprintln(w, "SAM MonoWatch: no cross-domain impact detected ✓")
}

// DaemonUnreachable prints the advisory-skip message.
func DaemonUnreachable(w io.Writer) {
	fmt.Fprintln(w, "SAM: MonoGraph unreachable, skipping impact check")
}
