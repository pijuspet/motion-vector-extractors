#include <stdio.h>
#include "writer.h"

#include <stdlib.h>


bool MotionVectorWriter::Open(std::string const& filename) {
    file.open(filename);
    if (!file.is_open()) {
        fprintf(stderr, "Failed to open file: %s\n", filename.c_str());
        return false;
    }

    file << "frame,method_id,source,w,h,src_x,src_y,dst_x,dst_y,flags,motion_x,motion_y,motion_scale\n";
    frame_num = 0; // Reset frame number
    return true;    
  
}

int MotionVectorWriter::Write(int frame_num, const AVMotionVector *mvs, int method_id, size_t size) {

    if (!file.is_open()) {
        fprintf(stderr, "File not open for writing\n");
        return -1;
    }

    if (!mvs) {
        fprintf(stderr, "Invalid motion vector\n");
        return -1;
    }
    
    // Write the motion vector data in CSV format
    if (frame_num < 0) {
        fprintf(stderr, "Invalid frame number: %d\n", frame_num);
        return -1;
    }

    // Write the motion vector data in CSV format
    if (frame_num < 0) {
        fprintf(stderr, "Invalid frame number: %d\n", frame_num);
        return -1;
    }

    for (int i = 0; i < size / sizeof(AVMotionVector); i++) {
        const AVMotionVector *mv = &mvs[i];
        
        if (mv->w <= 0 || mv->h <= 0) {
            fprintf(stderr, "Invalid motion vector dimensions: %d x %d\n", mv->w, mv->h);
            continue;
        }
        file << frame_num << "," << method_id << "," << mv->source << ","
            << int(mv->w) << "," << int(mv->h) << ","
            << mv->src_x << "," << mv->src_y << ","
            << mv->dst_x << "," << mv->dst_y << ","
            << "0x" << std::hex << mv->flags << ","
            << std::dec << mv->motion_x << ","
            << mv->motion_y << ","
            << mv->motion_scale << "\n";          
        }
    
    return 0; // Success    
}

void MotionVectorWriter::Close() {
    if (file.is_open()) {
        file.close();
    }   
}