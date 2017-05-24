
use bio::data_structures::fmindex::*;
use bio::data_structures::suffix_array::RawSuffixArray;
use bio::data_structures::fmindex::FMIndexable;

use std;
use std::collections::HashSet;
use std::cmp::{min,max};
use std::hash::Hash;

////////////////////////////////

use structs::run_config::*;
use structs::solutions::*;

use useful::*;

use algorithm_modes::kucherov::get_block_lengths;
use algorithm_modes::kucherov::candidate_condition;
use algorithm_modes::kucherov::filter_func;
pub static READ_ERR : u8 = b'N';

/*
This is the meat and potatoes of this program, the candidate generation step (AKA search step).
Given a pattern string (and some other information) and a config struct,
will ultimately return a set of candidate solutions.
These candidates store just enough necessary to indicate a specific "match" in the FMindex's text
Such matches are prefix-suffix overlaps between the pattern and the match location strings,
in the code the pattern is often referred to as 'a' and the match is referred to as 'b'
Depending on the config, candidates will be generated according to reversals, edit distance, inclusions etc.

In the literature, often the patterns are solved by way of numerous SUFFIX FILTERS and solved with
forwards search. This concept of direction is dependent on the frame of reference in this implementation.
Due to the backwards search of the FMIndex, the text string used for the BWT is actually backwards.
For this reason, much of the search, iteration etc. are all backwards
(with respect to left -> right or ascending indexes).
Relative to the actual strings inside the text, and conceptually, the search is still forwards.
These two equally-correct perspectives cannot be resolved in all cases, so when possible I use
terms that are general in both directions. ie: FILTER instead of SUFFIX FILTER
*/
pub trait GeneratesCandidates : FMIndexable {
    fn generate_candidates(&self,
                           pattern : &[u8],
                           config : &Config,
                           maps : &Maps,
                           id_a : usize,
                           sa : &RawSuffixArray,
                            ) -> HashSet<Candidate> {
//        println!("\nPATTERN {}", String::from_utf8_lossy(pattern));
        let patt_len = pattern.len();
        let block_lengths = get_block_lengths(patt_len as i32, config.err_rate, config.thresh);
        let mut candidate_set: HashSet<Candidate> = HashSet::new();
        let block_id_lookup = get_block_id_lookup(&block_lengths);
        let full_interval = Interval {
            lower: 0,
            upper: self.bwt().len() - 1,
        };
        let mut p_i : i32 = (patt_len-1) as i32;
        let patt_blocks : i32 = block_lengths.len() as i32;

        // each of these represents a suffix filter to be treated as a pattern to query the index
        //TODO split into PatternConstant and FilterConstant structs
//        println!("{:?}", &block_lengths);
        for (block_id, block_len) in block_lengths.iter().enumerate() {
//            println!("\nFILTER  {}", String::from_utf8_lossy(&pattern[..(p_i as usize + 1)]));
            //TODO how do i slice a slice?
            let cns = SearchConstants{
                pattern: pattern,
                hard_error_cap : (patt_len as f32 * config.err_rate).floor() as i32,
                config : config,
                maps : maps,
                block_id_lookup : &block_id_lookup,
                sa : sa,
                id_a : id_a,
                first_block_id : block_id as i32,
                patt_blocks : patt_blocks,
                blind_a_chars: patt_len - p_i as usize - 1,
                max_b_len : if config.edit_distance {(patt_len as f32 / (1.0-config.err_rate)).floor() as usize} else {patt_len},
            };

            //This begins the search and represents a single "query" for a single pattern filter
            self.recurse_candidates(
                &mut candidate_set, &cns, 0, p_i,
                LastOperation::Initial, 0, 0,
                &full_interval, &String::new());

            // the filters begin as the entire pattern, and gradually get shorter.
            p_i -= *block_len;
        }
        if candidate_set.is_empty(){
            if config.verbose {println!("OK no candidates found for '{}', skipping verification.",
                                        maps.get_name_for(id_a))};
        } else {
            if config.verbose {println!("OK finished candidates for '{}'.",
                                        maps.get_name_for(id_a))};
        }
        candidate_set
    }

    /*
    This conceptually corresponds to the search for one FILTER of the candidate.
    The call branches recursively as specified by the functions used for the algorithm mode.
    Various information that changes with each iteration is stored on the call stack directly.
    */
    fn recurse_candidates(&self,
                          cand_set : &mut HashSet<Candidate>,
                          cns : &SearchConstants,
                          errors : i32,
                          p_i : i32,
                          last_operation : LastOperation,
                          a_match_len : usize,
                          b_match_len : usize,
                          match_interval : &Interval,
                          debug : &str){
        if match_interval.lower > match_interval.upper{
            // range is inclusive on both ends within the walk.
            // empty range -> prune branch
            return
        }

        let completed_blocks : i32 = match cns.block_id_lookup.get(p_i as usize){
            //p_i corresponds with the index of the NEXT matched character. Upon matching the entire pattern,
            //this value can become -1. Here this match statement takes care of this special case
            Some(x) => x - cns.first_block_id,
            None    => cns.patt_blocks - cns.first_block_id,
        };
        //look up how many errors are allowed from the filter module
        let permitted_errors : i32 = min(cns.hard_error_cap, filter_func(completed_blocks, cns.patt_blocks));

        //Design decision: if the lengths of A and B differ, we are generous with the size for lookups
        let generous_match_len = std::cmp::max(a_match_len, b_match_len) + 1;
        let cand_condition_satisfied =
            candidate_condition(generous_match_len as i32, completed_blocks, cns.config.thresh, errors);

        if cand_condition_satisfied && last_operation.allows_candidates(){
            // Add candidates to set for matched b strings preceded by '$'
            let a = b'$';
            let less = self.less(a);
            let dollar_interval = Interval {
                lower : less + if match_interval.lower > 0 { self.occ(match_interval.lower - 1, a) } else { 0 },
                upper : less + self.occ(match_interval.upper, a),
            }; //final interval must have exclusive end
            let positions = dollar_interval.occ(cns.sa);
            add_candidates_from_positions(positions, cand_set, cns, a_match_len, b_match_len, debug, false);
        }

        let pattern_finished = p_i <= -1;
        if pattern_finished {
            // end of the pattern string
            // Add inclusion candidates to set at this position for everything in the remaining range
            if cns.config.inclusions && cand_condition_satisfied && last_operation.allows_candidates(){
                let inclusion_interval = Interval{
                    lower : match_interval.lower,
                    upper : match_interval.upper + 1,
                }; // final interval must have exclusive end
                let positions = inclusion_interval.occ(cns.sa);
                add_candidates_from_positions(positions, cand_set, cns, a_match_len, b_match_len, debug, true);
            }
            return;
            //nothing to do here.
        }

        // consider a new derived b string match, one char longer (in front) than existing match
        let p_char = *cns.pattern.get(p_i as usize).expect("THE P CHAR");
//        println!();
//        println!("{}", String::from_utf8_lossy(cns.pattern));
//        println!("{}|{}", cns.maps.push_string(&(p_char as char).to_string(), " ", p_i as usize), debug);
        for &a in cns.config.alphabet() {
            let less = self.less(a);
            let next_interval = Interval{
                lower : less + if match_interval.lower > 0 { self.occ(match_interval.lower - 1, a) } else { 0 },
                upper : less + self.occ(match_interval.upper, a) - 1,
            };

            //TODO remove debug stuff
            let recurse_errors =  if p_char == a && a != READ_ERR {errors} else {errors + 1};
            let debug_a = if p_char == a {a as char} else {smaller(a)};
            if recurse_errors <= permitted_errors {
                let next_debug = format!("{}{}", debug_a, debug);
                // recursively explore SUBSTITUTION cases (both hamming and levenshtein)
                self.recurse_candidates(cand_set,
                                        cns,
                                        recurse_errors,
                                        p_i-1,  //step left
                                        LastOperation::Substitution,
                                        a_match_len + 1,
                                        b_match_len + 1,
                                        &next_interval,
                                        &next_debug);
            }
            if (errors < permitted_errors) && cns.config.edit_distance && last_operation.allows_insertion() {
                if p_char != a{
                    // recursively explore INSERTION cases (if levenshtein)
                    let next_debug = format!("{}.{}", debug_a, debug);
                    self.recurse_candidates(cand_set,
                                            cns,
                                            errors + 1, //always induces an error
                                            p_i,        //don't step left
                                            LastOperation::Insertion,
                                            a_match_len,//the pattern string doesn't grow
                                            b_match_len + 1,
                                            &next_interval,
                                            &next_debug);
                }else{

//                    println!("{} insert prohibited", a as char);
                }


            }
        }

        if cns.config.edit_distance && errors < permitted_errors && !pattern_finished{
            // recursively explore DELETION cases (if levenshtein) and have at least 1 spare pattern char to jump over
            if last_operation.allows_deletion(){

                let next_debug = format!("{}{}", '_', debug);
                self.recurse_candidates(cand_set,
                                        cns,
                                        errors + 1,
                                        p_i - 1,         //one step without matching
                                        LastOperation::Deletion,
                                        a_match_len + 1,
                                        b_match_len,     //the matched string doesn't grow
                                        &match_interval, //stays unchanged
                                        &next_debug);
            }
        }
    }
}

fn smaller(a : u8) -> char{
    match a as char {
        'A' => 'a',
        'C' => 'c',
        'N' => 'n',
        'G' => 'g',
        'T' => 't',
        _ => '?',
    }
}

#[derive(PartialEq, Copy, Clone)]
pub enum LastOperation{
    Initial,
    Substitution,
    Insertion,
    Deletion,
}

/*
There are many ways to the same B match when insertions and deletions are allowed.
Restricting the branching attempts to avoid these redundant walks
*/
impl LastOperation{
    fn allows_deletion(self) -> bool{
        self == LastOperation::Deletion
        || self == LastOperation::Substitution
    }

    fn allows_insertion(self) -> bool{
        self == LastOperation::Insertion
            || self == LastOperation::Substitution
    }

    fn allows_candidates(self) -> bool{
        self == LastOperation::Initial
            || self == LastOperation::Substitution
    }
}

/*
given positions in the text (and various other data) determine which of these are suitable
locations to generate candidates. For each, add a new candidate to cand_set
*/
fn add_candidates_from_positions(positions : Vec<usize>,
                                 cand_set : &mut HashSet<Candidate>,
                                 cns : &SearchConstants, a_match_len : usize,
                                 b_match_len : usize, debug : &str, inclusion : bool){
    for mut position in positions {
        if !inclusion{
            //non-inclusions include the preceding dollar sign
            position += 1;
        }
        //from position, identify the b string we are dealing with. how we do so differs for inclusions
        let (id_b, index_b) = if inclusion {
            cns.maps.find_occurrence_containing(position)
        } else {
            (cns.maps.id_for(position), position)
        };

        if id_b == cns.id_a || (cns.config.edit_distance &&
                cns.id_a == companion_id(cns.id_a)){
            // matching self or partner. not interested in these solutions.
            continue;
        }

        if cns.config.edit_distance && cns.id_a > id_b{
            //don't need this candidate. A complementary candidate (that verifies to same solution)
            //will be found by a partner task for which id_a < id_b
            println!("DISCARDING");
            continue;
            //TODO ensure correctness!
        }
//        println!();
//        cns.maps.print_text_debug();
//        println!("{}{}", cns.maps.push_string(debug, " ", position), cns.maps.push_string("", "~", cns.blind_a_chars));

        let a_len = cns.pattern.len();
        let b_len = cns.maps.get_length(id_b);

        // a: [e1 | a2 ]
        // b:     [ b2 | b3]  for suff-pref overlap
        //
        // a:     [ a2 ]
        // b: [b1 | b2 | b3]  for inclusions

        //a1,a2,a3 are all derivable from known values
        let a2 = a_match_len + cns.blind_a_chars;
        let a1 = if inclusion {0} else {(a_len - a2) as i32};
        let a3 = a_len as i32 - a1 - (a2 as i32);
        //neither inclusions nor suff-pref overlap search processes should find cands where a3 > 0
        assert!(a3 == 0);

        let b1 = if inclusion {(position - index_b) as i32} else {0};
        //a1 and b2 can be represented as one value (a_left_overhang) but here they are divided
        //into a1 and b2 (with one always being zero) to help make the code more comprehensible.
        //candidates collapse a1 and b1 into this one value as storage space becomes a factor
        assert!(a1 * b1 == 0);
        let (min_b2, max_b2) = if !cns.config.edit_distance {
            //if hamming a2 == b2. So the possible values range from b2-->b2 (inclusively)
            (a2, a2)
        } else {
            // b_overlap_len is unknown, but it has upper and lower bounds as determined by the
            // length of b, the error rate etc.
            (
                max((a2 as f32 * (1.0-cns.config.err_rate)).ceil() as usize,
                    b_match_len),
                min((a2 as f32 / (1.0-cns.config.err_rate)).floor() as usize,
                    b_len),
            )
        };
        let possible_b2s = (min_b2)..(max_b2 + 1);

        // for edit distance, numerous instantiations of
        for b2 in possible_b2s{
            let b3 = b_len as i32 - b1 - (b2 as i32);
            if b3 < 0 {
                // b is too short to accommodate a suitable match length
                continue;
            }
            let mut new_debug = debug.to_owned();
            new_debug.push_str(&format!(" incl {} blind {}", inclusion, cns.blind_a_chars));
            let c = Candidate {
                id_b: id_b,
                overlap_a: a2,
                overlap_b: b2,
                overhang_left_a: a1 - b1,
                debug_str : new_debug,
            };
//            println!("{:#?}", &c);
            cand_set.insert(c);
        }
    }
}


#[derive(Debug)]
pub struct SearchConstants<'a>{
    config : &'a Config,
    maps : &'a Maps,

    block_id_lookup : &'a Vec<i32>,
    sa : &'a RawSuffixArray,
    pattern: &'a [u8],
    id_a : usize,
    blind_a_chars: usize,
    hard_error_cap : i32,

    max_b_len : usize,

    first_block_id : i32,
    patt_blocks : i32,
}

fn get_block_id_lookup(block_lengths : &[i32]) -> Vec<i32>{
    let mut lookup : Vec<i32> = Vec::new();
    for (id, block_length) in block_lengths.iter().enumerate() {
        for _ in 0..*block_length{
            lookup.push(id as i32);
        }
    }
    lookup.reverse();
    lookup.shrink_to_fit();
    lookup
}