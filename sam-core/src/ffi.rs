use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic;
use std::path::Path;
use std::time::Duration;

use crate::{git, monograph, profile, workspace};

// ---------------------------------------------------------------------------
// C-compatible types
// ---------------------------------------------------------------------------

/// Array of C strings. Caller must free with `sam_free_string_array`.
#[repr(C)]
pub struct SamStringArray {
    pub data: *mut *mut c_char,
    pub len: usize,
}

/// Domain info for FileProvider enumeration.
#[repr(C)]
pub struct SamDomainInfo {
    pub path: *mut c_char,
    pub is_hydrated: bool,
    pub file_count: i64, // -1 if unknown
}

/// Array of domain info entries. Caller must free with `sam_free_domain_info_array`.
#[repr(C)]
pub struct SamDomainInfoArray {
    pub data: *mut SamDomainInfo,
    pub len: usize,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn c_str_to_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().ok()
}

fn string_to_c(s: &str) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

fn strings_to_array(strings: Vec<String>) -> SamStringArray {
    let len = strings.len();
    let mut ptrs: Vec<*mut c_char> = strings
        .into_iter()
        .map(|s| CString::new(s).unwrap_or_default().into_raw())
        .collect();
    let data = ptrs.as_mut_ptr();
    std::mem::forget(ptrs);
    SamStringArray { data, len }
}

fn empty_string_array() -> SamStringArray {
    SamStringArray {
        data: std::ptr::null_mut(),
        len: 0,
    }
}

fn empty_domain_info_array() -> SamDomainInfoArray {
    SamDomainInfoArray {
        data: std::ptr::null_mut(),
        len: 0,
    }
}

// ---------------------------------------------------------------------------
// Memory management
// ---------------------------------------------------------------------------

/// Free a string returned by a SAM FFI function.
#[no_mangle]
pub extern "C" fn sam_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            drop(CString::from_raw(s));
        }
    }
}

/// Free a string array returned by a SAM FFI function.
#[no_mangle]
pub extern "C" fn sam_free_string_array(arr: SamStringArray) {
    if arr.data.is_null() || arr.len == 0 {
        return;
    }
    unsafe {
        let slice = std::slice::from_raw_parts(arr.data, arr.len);
        for &ptr in slice {
            if !ptr.is_null() {
                drop(CString::from_raw(ptr));
            }
        }
        // Reconstruct the Vec to free the pointer array itself
        let _ = Vec::from_raw_parts(arr.data, arr.len, arr.len);
    }
}

/// Free a domain info array returned by a SAM FFI function.
#[no_mangle]
pub extern "C" fn sam_free_domain_info_array(arr: SamDomainInfoArray) {
    if arr.data.is_null() || arr.len == 0 {
        return;
    }
    unsafe {
        let slice = std::slice::from_raw_parts(arr.data, arr.len);
        for info in slice {
            if !info.path.is_null() {
                drop(CString::from_raw(info.path));
            }
        }
        let _ = Vec::from_raw_parts(arr.data, arr.len, arr.len);
    }
}

// ---------------------------------------------------------------------------
// Repo operations
// ---------------------------------------------------------------------------

/// Find repo root from a path. Returns null if not found.
#[no_mangle]
pub extern "C" fn sam_find_repo_root(from: *const c_char) -> *mut c_char {
    let result = panic::catch_unwind(|| {
        let from_str = match c_str_to_str(from) {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        match profile::find_repo_root(Path::new(from_str)) {
            Ok(root) => string_to_c(&root.to_string_lossy()),
            Err(_) => std::ptr::null_mut(),
        }
    });
    result.unwrap_or(std::ptr::null_mut())
}

/// List all top-level domains in the repo (from git ls-tree).
/// Each entry includes hydration status and file count.
#[no_mangle]
pub extern "C" fn sam_list_domains(repo: *const c_char) -> SamDomainInfoArray {
    let result = panic::catch_unwind(|| {
        let repo_str = match c_str_to_str(repo) {
            Some(s) => s,
            None => return empty_domain_info_array(),
        };
        let repo_path = Path::new(repo_str);

        let dirs = match git::list_top_level_dirs(repo_path) {
            Ok(d) => d,
            Err(_) => return empty_domain_info_array(),
        };

        let ws_state = workspace::load(repo_path).unwrap_or_default();

        let mut infos: Vec<SamDomainInfo> = Vec::with_capacity(dirs.len());
        for dir in &dirs {
            let is_hydrated = ws_state.has_domain(dir);
            let file_count = git::count_tree_files(repo_path, dir)
                .map(|c| c as i64)
                .unwrap_or(-1);

            infos.push(SamDomainInfo {
                path: string_to_c(dir),
                is_hydrated,
                file_count,
            });
        }

        let len = infos.len();
        let data = infos.as_mut_ptr();
        std::mem::forget(infos);

        SamDomainInfoArray { data, len }
    });
    result.unwrap_or_else(|_| empty_domain_info_array())
}

/// Check if a specific domain is hydrated (has materialized files on disk).
#[no_mangle]
pub extern "C" fn sam_is_domain_hydrated(repo: *const c_char, domain: *const c_char) -> bool {
    let result = panic::catch_unwind(|| {
        let repo_str = match c_str_to_str(repo) {
            Some(s) => s,
            None => return false,
        };
        let domain_str = match c_str_to_str(domain) {
            Some(s) => s,
            None => return false,
        };
        let repo_path = Path::new(repo_str);
        workspace::load(repo_path)
            .map(|s| s.has_domain(domain_str))
            .unwrap_or(false)
    });
    result.unwrap_or(false)
}

/// Hydrate a domain (git sparse-checkout add). Returns 0 on success, -1 on error.
#[no_mangle]
pub extern "C" fn sam_hydrate_domain(repo: *const c_char, domain: *const c_char) -> i32 {
    let result = panic::catch_unwind(|| {
        let repo_str = match c_str_to_str(repo) {
            Some(s) => s,
            None => return -1,
        };
        let domain_str = match c_str_to_str(domain) {
            Some(s) => s,
            None => return -1,
        };
        let repo_path = Path::new(repo_str);

        // Add to sparse checkout
        if git::add_sparse(repo_path, &[domain_str.to_string()]).is_err() {
            return -1;
        }

        // Update workspace state
        let mut state = workspace::load(repo_path).unwrap_or_default();
        state.add_domain(domain_str);
        if workspace::save(repo_path, &state).is_err() {
            return -1;
        }

        0
    });
    result.unwrap_or(-1)
}

/// Hydrate a domain with its dependencies (via MonoGraph or fallback).
/// Returns the list of all hydrated domains.
#[no_mangle]
pub extern "C" fn sam_hydrate_domain_with_deps(
    repo: *const c_char,
    domain: *const c_char,
) -> SamStringArray {
    let result = panic::catch_unwind(|| {
        let repo_str = match c_str_to_str(repo) {
            Some(s) => s,
            None => return empty_string_array(),
        };
        let domain_str = match c_str_to_str(domain) {
            Some(s) => s,
            None => return empty_string_array(),
        };
        let repo_path = Path::new(repo_str);

        // Load profile config and repo config
        let profiles_config = profile::load_profiles(repo_path).ok();
        let repo_config = profile::load_repo_config(repo_path).unwrap_or_default();

        // Determine auto_include from profile if available
        let auto_include = profiles_config
            .as_ref()
            .and_then(|pc| {
                // Try to find a profile containing this domain
                pc.profiles.values().find(|p| match &p.domains {
                    profile::Domains::All => true,
                    profile::Domains::List(list) => list.iter().any(|d| d == domain_str),
                })
            })
            .map(|p| p.auto_include.clone())
            .unwrap_or_default();

        let ai_infer = profiles_config
            .as_ref()
            .and_then(|pc| {
                pc.profiles.values().find(|p| match &p.domains {
                    profile::Domains::All => true,
                    profile::Domains::List(list) => list.iter().any(|d| d == domain_str),
                })
            })
            .map(|p| p.ai_infer)
            .unwrap_or(true);

        // Try MonoGraph with a 2-second timeout
        let domains_to_hydrate = {
            let client =
                monograph::Client::new(&repo_config.monograph.address, Duration::from_secs(2));

            if client.health() {
                let req = monograph::ResolveRequest {
                    domains: vec![domain_str.to_string()],
                    auto_include: auto_include.clone(),
                    ai_infer,
                    cochange_commits: Some(repo_config.monograph.cochange_commits),
                    cochange_min_score: Some(repo_config.monograph.cochange_min_score),
                };
                match client.resolve(&req) {
                    Ok(resp) => resp.domains.into_iter().map(|d| d.path).collect::<Vec<_>>(),
                    Err(_) => {
                        // Fall back
                        let resp = monograph::fallback_resolve(
                            &[domain_str.to_string()],
                            &auto_include,
                        );
                        resp.domains.into_iter().map(|d| d.path).collect()
                    }
                }
            } else {
                // MonoGraph unreachable — fallback
                let resp =
                    monograph::fallback_resolve(&[domain_str.to_string()], &auto_include);
                resp.domains.into_iter().map(|d| d.path).collect()
            }
        };

        // Add all to sparse checkout
        if git::add_sparse(repo_path, &domains_to_hydrate).is_err() {
            return empty_string_array();
        }

        // Update workspace state
        let mut state = workspace::load(repo_path).unwrap_or_default();
        for d in &domains_to_hydrate {
            state.add_domain(d);
        }
        let _ = workspace::save(repo_path, &state);

        // Return all hydrated domains (from state, not just the new ones)
        strings_to_array(state.hydrated_domains)
    });
    result.unwrap_or_else(|_| empty_string_array())
}

/// Get the active profile name. Returns null if none is set.
#[no_mangle]
pub extern "C" fn sam_get_active_profile(repo: *const c_char) -> *mut c_char {
    let result = panic::catch_unwind(|| {
        let repo_str = match c_str_to_str(repo) {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        let repo_path = Path::new(repo_str);
        match workspace::load(repo_path) {
            Ok(state) => match state.active_profile {
                Some(ref name) => string_to_c(name),
                None => std::ptr::null_mut(),
            },
            Err(_) => std::ptr::null_mut(),
        }
    });
    result.unwrap_or(std::ptr::null_mut())
}

/// Get the list of hydrated domains.
#[no_mangle]
pub extern "C" fn sam_get_hydrated_domains(repo: *const c_char) -> SamStringArray {
    let result = panic::catch_unwind(|| {
        let repo_str = match c_str_to_str(repo) {
            Some(s) => s,
            None => return empty_string_array(),
        };
        let repo_path = Path::new(repo_str);
        match workspace::load(repo_path) {
            Ok(state) => strings_to_array(state.hydrated_domains),
            Err(_) => empty_string_array(),
        }
    });
    result.unwrap_or_else(|_| empty_string_array())
}

/// Count files in a domain (from git tree, not disk). Returns -1 on error.
#[no_mangle]
pub extern "C" fn sam_count_domain_files(repo: *const c_char, domain: *const c_char) -> i64 {
    let result = panic::catch_unwind(|| {
        let repo_str = match c_str_to_str(repo) {
            Some(s) => s,
            None => return -1,
        };
        let domain_str = match c_str_to_str(domain) {
            Some(s) => s,
            None => return -1,
        };
        let repo_path = Path::new(repo_str);
        git::count_tree_files(repo_path, domain_str)
            .map(|c| c as i64)
            .unwrap_or(-1)
    });
    result.unwrap_or(-1)
}

/// List tree entries under a path (for FileProvider enumeration).
/// Returns a JSON string: `[{"path":"...","kind":"tree|blob","mode":"...","hash":"..."}]`.
/// Returns null on error.
#[no_mangle]
pub extern "C" fn sam_list_tree_entries(
    repo: *const c_char,
    path: *const c_char,
) -> *mut c_char {
    let result = panic::catch_unwind(|| {
        let repo_str = match c_str_to_str(repo) {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        let path_str = match c_str_to_str(path) {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        let repo_path = Path::new(repo_str);

        match git::list_tree_entries(repo_path, path_str) {
            Ok(entries) => match serde_json::to_string(&entries) {
                Ok(json) => string_to_c(&json),
                Err(_) => std::ptr::null_mut(),
            },
            Err(_) => std::ptr::null_mut(),
        }
    });
    result.unwrap_or(std::ptr::null_mut())
}
