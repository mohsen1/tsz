var Mixed;
(function (Mixed) {
    Mixed[Mixed["A"] = 0] = "A";
    Mixed[Mixed["B"] = 5] = "B";
    Mixed[Mixed["C"] = 6] = "C";
})(Mixed || (Mixed = {}));
var m = Mixed.C;
