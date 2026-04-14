package internal

import (
	"testing"
)

// ─────────────────────────────────────────────────────────────────────────────
// dedup
// ─────────────────────────────────────────────────────────────────────────────

func TestDedup(t *testing.T) {
	tests := []struct {
		name  string
		input []string
		want  []string
	}{
		{
			name:  "no duplicates",
			input: []string{"a", "b", "c"},
			want:  []string{"a", "b", "c"},
		},
		{
			name:  "with duplicates preserves first occurrence order",
			input: []string{"b", "a", "b", "c", "a"},
			want:  []string{"b", "a", "c"},
		},
		{
			name:  "all same",
			input: []string{"x", "x", "x"},
			want:  []string{"x"},
		},
		{
			name:  "nil input",
			input: nil,
			want:  nil,
		},
		{
			name:  "empty",
			input: []string{},
			want:  []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := dedup(tt.input)
			if len(got) != len(tt.want) {
				t.Fatalf("dedup(%v) = %v, want %v", tt.input, got, tt.want)
			}
			for i := range tt.want {
				if got[i] != tt.want[i] {
					t.Errorf("dedup(%v)[%d] = %q, want %q", tt.input, i, got[i], tt.want[i])
				}
			}
		})
	}
}

// ─────────────────────────────────────────────────────────────────────────────
// splitLines
// ─────────────────────────────────────────────────────────────────────────────

func TestSplitLines(t *testing.T) {
	tests := []struct {
		name  string
		input string
		want  []string
	}{
		{
			name:  "newline-separated",
			input: "a/b.go\nc/d.go\ne/f.go",
			want:  []string{"a/b.go", "c/d.go", "e/f.go"},
		},
		{
			name:  "trailing newline",
			input: "a/b.go\n",
			want:  []string{"a/b.go"},
		},
		{
			name:  "blank lines skipped",
			input: "a/b.go\n\nc/d.go\n",
			want:  []string{"a/b.go", "c/d.go"},
		},
		{
			name:  "empty string",
			input: "",
			want:  nil,
		},
		{
			name:  "whitespace only",
			input: "  \n  \n",
			want:  nil,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := splitLines(tt.input)
			if len(got) != len(tt.want) {
				t.Fatalf("splitLines(%q) = %v, want %v", tt.input, got, tt.want)
			}
			for i := range tt.want {
				if got[i] != tt.want[i] {
					t.Errorf("[%d] = %q, want %q", i, got[i], tt.want[i])
				}
			}
		})
	}
}
