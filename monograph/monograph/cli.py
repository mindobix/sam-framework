"""Typer CLI — monograph serve / analyze / status commands."""

from __future__ import annotations

import sys
from pathlib import Path

import typer

app = typer.Typer(
    name="monograph",
    help="MonoGraph — SAM Framework dependency analysis daemon.",
    add_completion=False,
)


@app.command()
def serve(
    port: int = typer.Option(7474, help="Port to listen on."),
    host: str = typer.Option("127.0.0.1", help="Host to bind to."),
    repo: Path = typer.Option(
        None,
        help="Repo root to analyze on startup. "
        "If omitted, waits for POST /analyze to set one.",
    ),
    log_level: str = typer.Option("info", help="Uvicorn log level."),
) -> None:
    """Start the MonoGraph HTTP daemon."""
    from monograph.server import run

    repo_path = repo.resolve() if repo else None
    if repo_path and not repo_path.is_dir():
        typer.echo(f"Error: repo path does not exist: {repo_path}", err=True)
        raise typer.Exit(1)

    typer.echo(f"MonoGraph daemon starting on http://{host}:{port}")
    if repo_path:
        typer.echo(f"Repo: {repo_path}")

    run(host=host, port=port, repo_path=repo_path, log_level=log_level)


@app.command()
def analyze(
    repo: Path = typer.Argument(..., help="Repo root to analyze."),
    force: bool = typer.Option(False, "--force", help="Rebuild even if graph is fresh."),
) -> None:
    """Build (or rebuild) the dependency graph for a repo and save to .sam/graph.json."""
    import json

    from monograph.cache import is_graph_stale, load_graph
    from monograph.analyzer import build_graph_locked

    repo_path = repo.resolve()
    if not repo_path.is_dir():
        typer.echo(f"Error: {repo_path} is not a directory.", err=True)
        raise typer.Exit(1)

    if not force and not is_graph_stale(repo_path):
        typer.echo("Graph is up-to-date. Use --force to rebuild.")
        g = load_graph(repo_path)
        if g:
            typer.echo(
                f"Domains: {len(g.domains)}, edges: {len(g.edges)}, "
                f"generated: {g.generated_at}"
            )
        return

    typer.echo(f"Building dependency graph for {repo_path} ...")
    try:
        g = build_graph_locked(repo_path)
    except Exception as exc:
        typer.echo(f"Error: {exc}", err=True)
        raise typer.Exit(1)

    typer.echo(
        f"Done. {len(g.domains)} domains, {len(g.edges)} edges. "
        f"Saved to {repo_path / '.sam/graph.json'}"
    )


@app.command()
def status(
    url: str = typer.Option("http://127.0.0.1:7474", help="Daemon URL."),
) -> None:
    """Check if the MonoGraph daemon is running and report its status."""
    import urllib.error
    import urllib.request

    try:
        with urllib.request.urlopen(f"{url}/health", timeout=3) as resp:
            import json

            data = json.loads(resp.read())
            graph_ready = data.get("graph_ready", False)
            age = data.get("graph_age_seconds")
            repo = data.get("repo_path", "n/a")

            typer.echo(f"MonoGraph daemon: UP")
            typer.echo(f"  Repo:        {repo}")
            typer.echo(f"  Graph ready: {graph_ready}")
            if age is not None:
                typer.echo(f"  Graph age:   {age:.0f}s")
    except urllib.error.URLError:
        typer.echo("MonoGraph daemon: NOT RUNNING", err=True)
        typer.echo(f"  Start with: monograph serve --repo /path/to/repo", err=True)
        raise typer.Exit(1)


if __name__ == "__main__":
    app()
