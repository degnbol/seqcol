use phf::phf_map;


pub static ALPHABETS: phf::Map<&'static str, &'static str> = phf_map! {
    "dna" => "ACGT-",
    "rna" => "ACGU-",
    "nucl" => "ACGTU-",
    "aa" => "ARNDCQEGHILKMFPSTWYV-",
    "aax" => "ARNDCQEGHILKMFPSTWYVBZX-",
    "all" => "ACGTURNDQEHILKMFPSWYVBZX-",
    "dnanogap" => "ACGT",
    "rnanogap" => "ACGU",
    "nuclnogap" => "ACGTU",
    "aanogap" => "ARNDCQEGHILKMFPSTWYV",
    "aaxnogap" => "ARNDCQEGHILKMFPSTWYVBZX",
    "allnogap" => "ACGTURNDQEHILKMFPSWYVBZX",
};
