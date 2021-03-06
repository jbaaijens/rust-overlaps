use bio::alignment::distance::{hamming, levenshtein};


use std;
use std::cmp::max;
use std::collections::HashSet;

use crate::structs::solutions::{Candidate, Solution};
use crate::structs::run_config::{Config, Maps};
use crate::search;
use crate::useful::{relative_orientation, companion_id, for_reversed_string};


/*
Another major step in the program, the candidate verification step. (AKA the filter step)
This function returns a set of solutions, each of which corresponds to a candidate in the input set.
Only candidates that are found (somewhat naively) to have small enough error distances (as defined in config)
correspond with an output solution. Other candidates are "filtered" out.
*/
pub fn verify_all(id_a : usize, candidates : HashSet<Candidate>, config : &Config, maps : &Maps) -> HashSet<Solution> {
    let num_cands = candidates.len();
    let mut solution_set : HashSet<Solution> = HashSet::new();
    if num_cands == 0 {
        return solution_set;
    }
    for c in candidates {
        if let Some(solution) = verify(id_a, c, config, maps){
            solution_set.insert(solution);
        }
    }
    solution_set
}

/*
Returns a solution corresponding with the given candidate if appropriate.
This function performs the CHECK if the candidate verifies.

The index can generate candidates that come in two forms:
>Suff-pref overlaps
a: [a1|a2]     a3==0
b:    [b2|b3]  b1==0

>Inclusions
a:    [a2]     a1==a3==0
b: [b1|b2|b3]

where a1,a2...b3 correspond with the LENGTHS of chunks of the pattern and match strings respectively,
a2 and b2 are the overlapping sections, and a1,a3,b1,b3 are the lengths of parts before and after.
*/
pub fn verify(id_a : usize, c : Candidate, config : &Config, maps : &Maps) -> Option<Solution>{
    let a_len = maps.get_length(id_a);
    assert_eq!(c.a3(a_len), 0);
    //b3 is usize, so implicitly b3 >= 0
    let a_part : &[u8] = &maps.get_string(id_a)  [c.a1()..(c.a1()+c.a2())];
    let b_part : &[u8] = &maps.get_string(c.id_b)[c.b1()..(c.b1()+c.b2())];
    let k_limit = (config.err_rate*(max(c.overlap_a, c.overlap_b) as f32)).floor() as u32;

    let errors : u32 = if config.edit_distance{
        modified_levenshtein(a_part, b_part)
    }else{
        assert!(a_part.len() == b_part.len());
        hamming(a_part, b_part) as u32
    };
    if errors <= k_limit{
        Some(solution_from_candidate(c, id_a, errors, maps, config))
    }else{
        None
    }
}


/*
A custom levenshtein distance where the first and last characters of each overlap are forced to be substitutions
As such, if the incoming strings have lengths
*/
pub fn modified_levenshtein(a_part : &[u8], b_part : &[u8]) -> u32 {
    if a_part.len() == b_part.len() && a_part.len() <= 2{
        //case where strings are the same length, but are of length 0, 1 or 2 (no indels possible)
        let mut errs = 0;
        if a_part.len() >= 1 {
            errs += error_at_pos_in_both(a_part, b_part, true);
        }
        if a_part.len() >= 2 {
            errs += error_at_pos_in_both(a_part, b_part, false);
        }
        return errs;
    }
    if a_part.len() < 2 || b_part.len() < 2{
        // undefined distance. return max possible value
        return std::u32::MAX;
    }
    //below this line: a_overlap_end >= 2 && b_overlap_end >= 2
    let first_char_err = error_at_pos_in_both(a_part, b_part, true);
    let last_char_err = error_at_pos_in_both(a_part, b_part, false);
    levenshtein(&a_part[1..a_part.len()-1], &b_part[1..b_part.len()-1])
        + first_char_err + last_char_err
}

#[inline]
fn error_at_pos_in_both(a_part : &[u8], b_part : &[u8], first : bool) -> u32 {
    assert!(a_part.len() >= 1);
    assert!(b_part.len() >= 1);
    let a_ind = if first {0} else {a_part.len()-1};
    let b_ind = if first {0} else {b_part.len()-1};
    if a_part[a_ind] != b_part[b_ind] {
        1
    } else {
        if a_part[a_ind] == search::READ_ERR { 1 } else { 0 }
    }
}

/*
Translates the input Candidate to a Solution.
This function does NOT check whether the input candidate is for a real solution.

This is one of the most confusing parts of the entire program, as everything is reversed
several times and it gets hard to keep track of how many times something is flipped.
Solutions correspond exactly with the EXTERNAL representations of the input strings,
but Candidates are largely INTERNAL (as verifying them requires the use of the index text).

*See annotation for verify() above for an explanation of a1,a2,a3,b1,b2,b3 etc. used here.
*/
fn solution_from_candidate(c : Candidate, id_a : usize, errors : u32,
                           maps : &Maps, config : &Config) -> Solution {
    let a_len = maps.get_length(id_a);
    let b_len = maps.get_length(c.id_b);
    let orientation = relative_orientation(id_a, c.id_b, config.reversals);
    let mut sol = Solution{
        id_a : id_a,
        id_b : c.id_b,
        orientation : orientation,
        overlap_a : c.overlap_a,
        overlap_b : c.overlap_b,
        overhang_left_a : c.overhang_left_a,
        overhang_right_b : (c.b3(b_len) as i32) - (c.a3(a_len) as i32),
        errors : errors,
    };
    translate_solution_to_external(&mut sol, config, maps);
    sol
}

#[inline]
fn id_order_ok(sol : &Solution, maps : &Maps) -> bool {
    maps.get_name_for(sol.id_a).
        cmp(maps.get_name_for(sol.id_b))
        != std::cmp::Ordering::Greater
}

fn translate_solution_to_external(sol : &mut Solution, config : &Config, maps : &Maps){
    assert!(sol.id_a != sol.id_b);
    if config.reversals {
        assert!(sol.id_a != companion_id(sol.id_b, config.reversals));
    }

    if !(id_order_ok(sol, maps)) {
        sol.v_flip();
    }
    assert!(id_order_ok(sol, maps));

    if config.reversals {
        if for_reversed_string(sol.id_a){
            sol.h_flip(config.reversals);
        }
        assert!(!for_reversed_string(sol.id_a));
    }

    sol.mirror_horizontally(); //finally, compensate for the index being entirely backwards
    assert!(!config.reversals || sol.id_a % 2 == 0);
}
