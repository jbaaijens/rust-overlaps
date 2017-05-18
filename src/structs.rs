
pub mod solutions{
    use std::hash::{Hash, Hasher};
    use std::cmp::Ordering;

    #[derive(Hash,PartialEq, Eq, Debug, PartialOrd, Ord)]
    pub enum Orientation{
        Normal,
        Reversed,
    }

    //NOT oriented
    #[derive(Hash,PartialEq, Eq, Debug)]
    pub struct Candidate{
        pub id_b : usize,
        pub overlap_a : usize,
        pub overlap_b : usize,
        pub overhang_left_a : i32,
        pub debug_str : String,
    }

    //oriented
    #[derive(Debug)]
    pub struct Solution{
        pub id_a : usize,
        pub id_b : usize,
        pub orientation : Orientation,
        pub overhang_left_a : i32,
        pub overhang_right_b : i32,
        pub overlap_a : usize,
        pub overlap_b : usize,
        pub errors : u32,
        pub cigar : String,
    }

    impl Ord for Solution {
        fn cmp(&self, other: &Self) -> Ordering {
            (self.id_a, self.id_b, &self.orientation, self.overhang_left_a, self.overhang_right_b, self.overlap_a, self.overlap_b)
                .cmp(&(other.id_a, other.id_b, &other.orientation, other.overhang_left_a, other.overhang_right_b, other.overlap_a, other.overlap_b))
        }
    }

    impl PartialOrd for Solution {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    impl PartialEq for Solution {
        fn eq(&self, other: &Self) -> bool {
            (self.id_a, self.id_b, &self.orientation, self.overhang_left_a, self.overhang_right_b, self.overlap_a, self.overlap_b)
                == (other.id_a, other.id_b, &other.orientation, other.overhang_left_a, other.overhang_right_b, other.overlap_a, other.overlap_b)
        }
    }

    impl Eq for Solution { }

    impl Hash for Solution {
        fn hash<H: Hasher>(&self, state: &mut H) {
            self.id_a.hash(state);
            self.id_b.hash(state);
            self.orientation.hash(state);
            self.overlap_a.hash(state);
            self.overlap_b.hash(state);
            self.overhang_left_a.hash(state);
            self.overhang_right_b.hash(state);
            // ERRORS and CIGAR not need to contribute to uniqueness
        }
    }
}

pub mod run_config{
    extern crate bidir_map;
    use bidir_map::BidirMap;

    #[derive(Debug)]
    pub struct Maps{
        pub text : Vec<u8>,
        pub id2name_vec : Vec<String>,
        pub id2index_bdmap : BidirMap<usize, usize>,
        pub num_ids : usize,
    }

    impl Maps{
        pub fn get_string(&self, id : usize) -> &[u8]{
            assert!(id < self.num_ids);
            &self.text[*self.id2index_bdmap.get_by_first(&id).expect("GAH")..self.get_end_index(id)]
        }

        pub fn get_length(&self, id : usize) -> usize{
            assert!(id < self.num_ids);
            self.get_end_index(id) - self.id2index_bdmap.get_by_first(&id).expect("WOO")
        }

        fn get_end_index(&self, id : usize) -> usize{
            assert!(id < self.num_ids);
            if id == self.num_ids-1{
                self.text.len() - 1 //$s in front. one # at the end
            }else{
                self.id2index_bdmap.get_by_first(&(id + 1)).expect("WAHEY") - 1
            }
        }

        //returns (id, index)
        pub fn find_occurrence_containing(&self, index : usize) -> (usize, usize){
            let mut best = (0, 1);
            for &(id, ind) in self.id2index_bdmap.iter(){
                if ind <= index && ind > best.1{
                    best = (id, ind);
                }
            }
            best
        }

        pub fn get_name_for(&self, id : usize) -> &str {
            self.id2name_vec.get(id).expect("get name")
        }

        pub fn print_text_debug(&self){
            println!("{}", String::from_utf8_lossy(&self.text));
        }

        pub fn spaces(&self, num : i32) -> String{
            let mut s = String::new();
            for _ in 0..num{
                s.push(' ');
            }
            s
        }

        pub fn formatted(&self, id : usize) -> String{
            format!("{}",String::from_utf8_lossy(self.get_string(id)))
        }

        pub fn tildes(&self, num : i32) -> String{
            let mut s = String::new();
            for _ in 0..num{
                s.push('~');
            }
            s
        }
    }

    #[derive(Debug)]
    pub struct Config{
        //TODO benchmark argument

        //required
        pub input : String,
        pub output : String,
        pub err_rate : f32,
        pub thresh : i32,
        pub worker_threads: usize,

        //optional
        pub sorted : bool,
        pub reversals : bool,
        pub inclusions : bool,
        pub edit_distance : bool,
        pub verbose : bool,
        pub time: bool,
        pub print: bool,
        pub n_alphabet: bool,
    }
}
